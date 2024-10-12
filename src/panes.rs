use std::collections::BTreeMap;

use crossterm::style::Color;

use crate::{components::scroll_view::ScrollView, document::{Document, DocumentId}, editor::Mode, ui::{buffer::Buffer, Rect}, NonZeroIncrementalId};

type PaneId = NonZeroIncrementalId;

const GUTTER_LINE_NUM_PAD_LEFT: u16 = 2;
const GUTTER_LINE_NUM_PAD_RIGHT: u16 = 1;
const MIN_GUTTER_WIDTH: u16 = 6;

fn gutter_and_document_areas(size: Rect, doc: &Document) -> (Rect, Rect) {
    let gutter_width = doc
        .text
        .rope
        .line_len()
        .checked_ilog10()
        .unwrap_or(1) as u16
        + 1
        + GUTTER_LINE_NUM_PAD_LEFT
        + GUTTER_LINE_NUM_PAD_RIGHT;
    let gutter_width = gutter_width.max(MIN_GUTTER_WIDTH);
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


#[derive(Default)]
enum Layout {
    #[default]
    Vertical,
    Horizontal,
}

pub struct Panes {
    root_id: PaneId,
    area: Rect,
    pub focused_id: PaneId,
    next_pane_id: PaneId,
    pub panes: BTreeMap<PaneId, Pane>,
}

impl Panes {
    pub fn new(area: Rect) -> Self {
        // remove 1 row for status line
        let area = area.clip_bottom(1);

        let pane_id = PaneId::default();
        let pane = Pane::new(area);
        let mut panes = BTreeMap::new();
        panes.insert(pane_id, pane);

        Self {
            area,
            panes,
            next_pane_id: pane_id.next(),
            root_id: pane_id,
            focused_id: pane_id,
        }
    }

    pub fn resize(&mut self, _area: Rect) {
        // recalc size for each pane
    }
}

#[derive(Default)]
pub struct Pane {
    pub area: Rect,
    pub doc_id: DocumentId,
    layout: Layout,
    parent_id: Option<PaneId>,
    child_id: Option<PaneId>,
    pub scroll_view: ScrollView,
}

impl Pane {
    // This will always point to doc_id 1 (the default)
    // use other builder methods to create non-default panes
    fn new(area: Rect) -> Self {
        Self {
            area,
            ..Default::default()
        }
    }

    pub fn render(&mut self, buffer: &mut Buffer, doc: &Document, mode: &Mode) {
        let (gutter_area, document_area) = gutter_and_document_areas(self.area, doc);

        (self.scroll_view.offset_x, self.scroll_view.offset_y) = compute_offset(document_area);

        self.render_document(document_area, buffer, doc);
        self.render_gutter(gutter_area, buffer, doc, mode);
    }

    fn render_document(&mut self, area: Rect, buffer: &mut Buffer, doc: &Document) {
        self.scroll_view.render(
            area,
            buffer,
            &doc.text,
            |buf: &mut Buffer, (x, y)| {
                // render trailing whitespace
                buf.put_symbol("~", x, y, Color::DarkGrey, Color::Reset);
            },
        );
    }

    fn render_gutter(&self, area: Rect, buffer: &mut Buffer, doc: &Document, mode: &Mode) {
        let max = doc.text.rope.line_len();

        for y in area.top()..=area.bottom() {
            let line_no = y as usize + self.scroll_view.scroll_y + 1;
            if line_no > max {
                break;
            }

            match mode {
                Mode::Insert | Mode::Replace => {
                    let label = format!(
                        "{: >1$}",
                        line_no,
                        area.width.saturating_sub(GUTTER_LINE_NUM_PAD_RIGHT) as usize
                    );
                    let fg = if line_no == doc.text.cursor_y + 1 {
                        Color::White
                    } else {
                        Color::DarkGrey
                    };
                    buffer.put_str(&label, 0, y, fg, Color::Reset);
                }
                Mode::Normal => {
                    let rel_line_no = self.scroll_view.cursor_position.y as isize - y as isize;
                    let (fg, label) = if rel_line_no == 0 {
                        (
                            Color::White,
                            format!("  {}", doc.text.cursor_y + 1),
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
                    buffer.put_str(&label, 0, y, fg, Color::Reset);
                }
            }
        }
    }
}
