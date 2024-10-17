use std::collections::{BTreeMap, HashMap};

use crossterm::style::Color;
use log::debug;

use crate::{components::scroll_view::ScrollView, document::{Document, DocumentId}, editor::Mode, ui::{borders::{Stroke, Symbol}, buffer::Buffer, Rect}, IncrementalId};

type PaneId = IncrementalId;
type NodeId = IncrementalId;

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

pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug)]
pub enum Layout {
    Vertical,
    Horizontal,
}

#[derive(Debug)]
pub struct Panes {
    pub focused_id: PaneId,
    pub panes: BTreeMap<PaneId, Pane>,
    area: Rect,
    next_pane_id: PaneId,
    next_node_id: NodeId,
    nodes: BTreeMap<NodeId, Node>,
}

#[derive(Debug)]
struct Node {
    id: NodeId,
    parent_id: Option<NodeId>,
    content: Content,
}

impl Node {
    fn root() -> Self {
        Self { id: NodeId::default(), parent_id: None, content: Content::Pane(PaneId::default()) }
    }
}

#[derive(Debug)]
enum Content {
    Pane(PaneId),
    Container(Container)
}

#[derive(Debug)]
struct Container {
    layout: Layout,
    area: Rect,
    children: Vec<NodeId>
}

impl Panes {
    pub fn new(area: Rect) -> Self {
        let mut panes = BTreeMap::new();
        let mut nodes = BTreeMap::new();
        let pane = Pane::root(area);
        let node = Node::root();
        let focused_id = pane.id;
        let next_pane_id = pane.id.next();
        let next_node_id = node.id.next();
        panes.insert(pane.id, pane);
        nodes.insert(node.id, node);

        Self {
            area,
            panes,
            nodes,
            next_pane_id,
            next_node_id,
            focused_id,
        }
    }

    pub fn resize(&mut self, new_size: Rect) {
        // recalc size for each pane, only if size has actually changed
        if new_size != self.area {
            // do the recalc...
        }
    }

    pub fn draw_borders(&mut self, buffer: &mut Buffer) {
        let mut map = HashMap::new();

        for (_, pane) in self.panes.iter() {
            if pane.area.right() < self.area.right() {
                // draw right borders
                let from = pane.area.top().saturating_sub(1);
                let to = (pane.area.bottom() + 1).min(self.area.bottom());
                let x = pane.area.right();

                for y in from..to {
                    let symbol = match map.get(&(x, y)) {
                        None => Symbol::Vertical,
                        Some(s) => match s {
                            Symbol::Horizontal => {
                                debug!("existing {:?}", s);
                                if y == pane.area.top().saturating_sub(1) {
                                    Symbol::HorizontalDown
                                } else if y == pane.area.bottom().saturating_sub(1) {
                                    Symbol::HorizontalUp
                                } else {
                                    Symbol::VerticalRight
                                }
                            },
                            _ => {
                                debug!("existing {:?}", s);
                                *s
                            }
                        }
                    };

                    map.insert((x, y), symbol);

                    buffer.put_symbol(symbol.as_str(Stroke::Thick), x, y, Color::DarkGrey, Color::Reset);
                }
            }

            if pane.area.bottom() < self.area.bottom() {
                // draw bottom borders
                let from = pane.area.left().saturating_sub(1);
                let to = (pane.area.right() + 1).min(self.area.right());
                let y = pane.area.bottom();

                for x in from..to {
                    let symbol = match map.get(&(x, y)) {
                        None => Symbol::Horizontal,
                        Some(s) => match s {
                            Symbol::Vertical => {
                                if x == pane.area.left().saturating_sub(1) {
                                    Symbol::VerticalRight
                                } else {
                                    Symbol::VerticalLeft
                                }
                            },
                            Symbol::VerticalLeft => Symbol::Cross,
                            _ => *s
                        }
                    };

                    map.insert((x, y), symbol);

                    buffer.put_symbol(symbol.as_str(Stroke::Thick), x, y, Color::DarkGrey, Color::Reset);
                }
            }
        }
    }

    pub fn close(&mut self, id: PaneId) {
        let pane = self.panes.get(&id).expect("Cannot get pane to close");

    }

    pub fn split(&mut self, layout: Layout) {
        let new_pane_id = self.next_pane_id.advance();
        let new_pane_node_id = self.next_node_id.advance();
        let new_parent_node_id = self.next_node_id.advance();

        let focused = self.panes.get_mut(&self.focused_id).unwrap();
        let node = self.nodes.get_mut(&focused.node_id).unwrap();


        // Create a new "pane" node to hold our new split
        let new_pane_node = Node {
            id: new_pane_node_id,
            parent_id: Some(node.id),
            content: Content::Pane(new_pane_id),
        };

        // Create a new parent node for the new pane node
        // and the original node that holds the focused pane
        let new_parent = Node {
            id: new_parent_node_id,
            parent_id: node.parent_id,
            content: Content::Container(
                Container {
                    layout,
                    area: focused.area,
                    children: vec![focused.node_id, new_pane_node.id]
                }
            ),
        };

        // remember to set the old focused node's parent
        // to the newly created parent
        node.parent_id = Some(new_parent.id);

        let (old_area, new_area) = match layout {
            Layout::Vertical => focused.area.split_vertically(1),
            Layout::Horizontal => focused.area.split_horizontally(1),
        };

        focused.area = old_area;

        let new_pane = Pane {
            id: new_pane_id,
            node_id: new_pane_node.id,
            doc_id: focused.doc_id,
            area: new_area,
            view: ScrollView::default()
        };


        self.panes.insert(new_pane_id, new_pane);
        self.nodes.insert(new_pane_node.id, new_pane_node);
        self.nodes.insert(new_parent.id, new_parent);

        self.focused_id = new_pane_id;
    }

    pub fn switch(&mut self, direction: Direction) {
        let focused = &self.panes[&self.focused_id];
        match direction {
            Direction::Up => {
                for (id, pane) in self.panes.iter() {
                    if pane.area.bottom() + 1 == focused.area.top() {
                        self.focused_id = *id
                    }
                }
            },
            Direction::Down => {
                for (id, pane) in self.panes.iter() {
                    if focused.area.bottom() + 1 == pane.area.top() {
                        self.focused_id = *id
                    }
                }
            },
            Direction::Left => {
                for (id, pane) in self.panes.iter() {
                    if pane.area.right() + 1 == focused.area.left() {
                        self.focused_id = *id
                    }
                }
            },
            Direction::Right => {
                for (id, pane) in self.panes.iter() {
                    if focused.area.right() + 1 == pane.area.left() {
                        self.focused_id = *id
                    }
                }
            },
        }
        debug!("Focused: {}", self.focused_id.0);
    }
}

#[derive(Debug)]
pub struct Pane {
    id: PaneId,
    node_id: NodeId,
    pub doc_id: DocumentId,
    pub area: Rect,
    pub view: ScrollView,
}

impl Pane {
    // This will always point to doc_id 1 (the default)
    // and have a default id of 1
    // and have the root node (1)
    // Use split to create subsequent panes
    fn root(area: Rect) -> Self {
        Self {
            id: PaneId::default(),
            area,
            doc_id: DocumentId::default(),
            view: ScrollView::default(),
            node_id: NodeId::default(),
        }
    }

    pub fn render(&mut self, buffer: &mut Buffer, doc: &Document, mode: &Mode, active: bool) {
        let (gutter_area, document_area) = gutter_and_document_areas(self.area, doc);

        (self.view.offset_x, self.view.offset_y) = compute_offset(document_area);

        self.render_document(document_area, buffer, doc);
        self.render_gutter(gutter_area, buffer, doc, mode, active);
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
