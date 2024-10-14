use std::collections::BTreeMap;

use crossterm::style::Color;

use crate::{components::scroll_view::ScrollView, document::{Document, DocumentId}, editor::Mode, ui::{borders::{HORIZONTAL, VERTICAL}, buffer::Buffer, Rect}, NonZeroIncrementalId};

type PaneId = NonZeroIncrementalId;

const GUTTER_LINE_NUM_PAD_LEFT: u16 = 2;
const GUTTER_LINE_NUM_PAD_RIGHT: u16 = 1;
const MIN_GUTTER_WIDTH: u16 = 6;

fn gutter_and_document_areas(size: Rect, doc: &Document) -> (Rect, Rect) {
    let gutter_width = doc
        .rope
        .line_len()
        .checked_ilog10()
        .unwrap_or(1) as u16
        + 1
        + GUTTER_LINE_NUM_PAD_LEFT
        + GUTTER_LINE_NUM_PAD_RIGHT;
    let gutter_width = gutter_width.max(MIN_GUTTER_WIDTH);

    // why do we clip bottom here?
    let gutter_area = size
        .clip_bottom(1)
        .clip_right(size.width.saturating_sub(gutter_width));

    let area = size.clip_left(gutter_area.width);

    (gutter_area, area)
}

fn compute_offset(size: Rect) -> (usize, usize) {
    (
        ((size.width as usize).saturating_sub(1).max(1) / 2).min(6),
        ((size.height as usize).saturating_sub(1).max(1) / 2).min(4),
    )
}


#[derive(Clone, Copy)]
pub enum Layout {
    Vertical,
    Horizontal,
}

pub struct Panes {
    area: Rect,
    pub focused_id: PaneId,
    next_pane_id: PaneId,
    pub panes: BTreeMap<PaneId, Pane>,
}

struct Node {
    parent_id: PaneId,
    content: Content,
}

enum Content {
    Pane(PaneId),
    Container(Container)
}

struct Container {
    layout: Layout,
    childern: Vec<PaneId>
}

impl Panes {
    pub fn new(area: Rect) -> Self {
        let pane_id = PaneId::default();
        let pane = Pane::new(area);
        let mut panes = BTreeMap::new();
        panes.insert(pane_id, pane);

        Self {
            area,
            panes,
            next_pane_id: pane_id.next(),
            focused_id: pane_id,
        }
    }

    pub fn resize(&mut self, _area: Rect) {
        // recalc size for each pane
    }

    pub fn close(&mut self, id: PaneId) {
        let pane = self.panes.get(&id).expect("Cannot get pane to close");

    }

    pub fn split(&mut self, layout: Layout) {
        let id = self.next_pane_id;
        let pane = self.focused().split(layout, id);
        self.panes.insert(id, pane);
        self.focused_id = id;
        self.next_pane_id.advance();
    }

    pub fn focused(&mut self) -> &mut Pane {
        self.panes.get_mut(&self.focused_id).expect("Cannot get focused pane")
    }
}

pub struct Pane {
    id: PaneId,
    pub area: Rect,
    pub doc_id: DocumentId,
    layout: Layout,
    parent_id: Option<PaneId>,
    pub view: ScrollView,
}

impl Pane {
    // This will always point to doc_id 1 (the default)
    // and have a default id of 1
    // Use split to create subsequent panes
    fn new(area: Rect) -> Self {
        Self {
            id: PaneId::default(),
            area,
            doc_id: DocumentId::default(),
            layout: Layout::Vertical,
            parent_id: None,
            view: ScrollView::default(),
        }
    }

    fn split(&mut self, layout: Layout, id: PaneId) -> Self {
        // we have to subtract 1 for border, which we always take from the parent
        let area = match layout {
            Layout::Vertical => {
                let new_area = self.area.clip_left((self.area.width + 1) / 2);
                self.area = self.area.clip_right((self.area.width + 2) / 2);
                new_area
            },
            Layout::Horizontal => {
                let new_area = self.area.clip_top((self.area.height + 1) / 2);
                self.area = self.area.clip_bottom((self.area.height + 2) / 2);
                new_area
            }
        };

        self.layout = layout;

        Self {
            id,
            area,
            doc_id: self.doc_id,
            layout: self.layout,
            parent_id: Some(self.id),
            view: ScrollView::default(),
        }
    }

    pub fn render(&mut self, buffer: &mut Buffer, doc: &Document, mode: &Mode, active: bool) {
        let (gutter_area, document_area) = gutter_and_document_areas(self.area, doc);

        (self.view.offset_x, self.view.offset_y) = compute_offset(document_area);

        self.render_document(document_area, buffer, doc);
        self.render_gutter(gutter_area, buffer, doc, mode, active);
    }

    fn render_border(&self, buffer: &mut Buffer) {
        match self.layout {
            Layout::Vertical => {
                for y in self.area.top()..=self.area.bottom() {
                    buffer.put_symbol(VERTICAL, self.area.right(), y, Color::DarkGrey, Color::Reset);
                }
            },
            Layout::Horizontal => {
                for x in self.area.left()..=self.area.right() {
                    buffer.put_symbol(HORIZONTAL, x, self.area.bottom(), Color::DarkGrey, Color::Reset);
                }
            },
        }
    }

    fn render_document(&mut self, area: Rect, buffer: &mut Buffer, doc: &Document) {
        self.view.render(
            area,
            buffer,
            &doc.rope,
            |buf: &mut Buffer, (x, y)| {
                // render trailing whitespace
                buf.put_symbol("~", x, y, Color::DarkGrey, Color::Reset);
            },
        );
    }

    fn render_gutter(&self, area: Rect, buffer: &mut Buffer, doc: &Document, mode: &Mode, active: bool) {
        fn absolute(line_no: usize, y: u16, area: Rect, buffer: &mut Buffer, view: &ScrollView) {
            let label = format!(
                "{: >1$}",
                line_no,
                area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
            );
            let fg = if line_no == view.text_cursor_y + 1 {
                Color::White
            } else {
                Color::DarkGrey
            };
            buffer.put_str(&label, area.left(), y, fg, Color::Reset);
        }

        fn relative(y: u16, area: Rect, buffer: &mut Buffer, view: &ScrollView) {
            let rel_line_no = view.view_cursor_position.y as isize - y as isize;
            let (fg, label) = if rel_line_no == 0 {
                (
                    Color::White,
                    format!("  {}", view.text_cursor_y + 1),
                )
            } else {
                (
                    Color::DarkGrey,
                    format!(
                        "{: >1$}",
                        rel_line_no.abs(),
                        area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
                    ),
                )
            };
            buffer.put_str(&label, area.left(), y, fg, Color::Reset);
        }

        let max = doc.rope.line_len();

        for y in 0..=area.height {
            let line_no = y as usize + self.view.scroll_y + 1;

            if line_no > max {
                break;
            }

            if active {
                match mode {
                    Mode::Insert | Mode::Replace =>
                        absolute(line_no, y + area.top(), area, buffer, &self.view),
                    Mode::Normal =>
                        relative(y + area.top(), area, buffer, &self.view)
                }
            } else {
                absolute(line_no, y + area.top(), area, buffer, &self.view);
            }
        }
    }
}
