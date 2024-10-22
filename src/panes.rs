use std::collections::{BTreeMap, HashMap};

use crossterm::style::Color;

use crate::{components::scroll_view::ScrollView, document::{Document, DocumentId}, editor::Mode, gutter, ui::{borders::{Stroke, Symbol}, buffer::Buffer, Rect}, IncrementalId};

type PaneId = IncrementalId;
type NodeId = IncrementalId;

fn find_and_intersect_with(symbol: Symbol, x: u16, y: u16, existing: &mut HashMap<(u16, u16), Symbol>) {
    let sym = match existing.get(&(x, y)) {
        None => symbol,
        Some(s) => s.intersect(symbol),
    };

    existing.insert((x, y), sym);
}

pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layout {
    Vertical,
    Horizontal,
}

#[derive(Debug)]
pub struct Panes {
    pub focus: PaneId,
    pub panes: BTreeMap<PaneId, Pane>,
    area: Rect,
    root: Node,
    next_pane_id: PaneId,
    next_node_id: NodeId,
}

#[derive(Debug)]
struct Node {
    id: NodeId,
    parent_id: Option<NodeId>,
    content: Content,
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
    children: Vec<Node>,
}

impl Node {
    fn pane_id(&self) -> PaneId {
        match self.content {
            Content::Pane(pid) => pid,
            _ => unreachable!(),
        }
    }

    fn layout(&self) -> Layout {
        match &self.content {
            Content::Container(cn) => cn.layout,
            _ => unreachable!(),
        }
    }

    fn area(&self) -> Rect {
        match &self.content {
            Content::Container(cn) => cn.area,
            _ => unreachable!()
        }
    }

    // panics when given a PaneId that doesn't exist
    // or no node points to
    fn find_by_pane_id(&mut self, pane_id: PaneId) -> &mut Self {
        let mut stack = vec![self];

        while let Some(node) = stack.pop() {
            match node.content {
                Content::Pane(pid) => {
                    if pid == pane_id { return node }
                },
                Content::Container(ref mut cn) => {
                    for child in cn.children.iter_mut() {
                        stack.push(child);
                    }
                },
            }
        }

        panic!("No node found that points to pane_id: {:?}", pane_id);
    }

    // panics when given a NodeId that doesn't exist
    fn find(&mut self, id: NodeId) -> &mut Self {
        let mut stack = vec![self];

        while let Some(node) = stack.pop() {
            if id == node.id {
                return node
            } else if let Content::Container(ref mut cn) = node.content {
                for child in cn.children.iter_mut() {
                    stack.push(child);
                }
            }
        }

        panic!("No node found with id: {:?}", id);
    }

    fn convert_to_container(&mut self, new_node_id: NodeId, layout: Layout, area: Rect) {
        self.content = Content::Container(Container {
            layout,
            area,
            children: vec![Node { id: new_node_id, parent_id: Some(self.id), content: Content::Pane(self.pane_id()) }]
        });
    }

    fn insert_pane_child_at(&mut self, id: NodeId, pane_id: PaneId, position: usize) {
        debug_assert!(matches!(self.content, Content::Container(_)));

        let child = Node {
            id,
            parent_id: Some(self.id),
            content: Content::Pane(pane_id)
        };

        match self.content {
            Content::Pane(_) => unreachable!(),
            Content::Container(ref mut cn) => cn.children.insert(position, child),
        }
    }

    fn child_position_by_pane_id(&self, pane_id: PaneId) -> usize {
        match &self.content {
            Content::Pane(_) => unreachable!(),
            Content::Container(c) => {
                let mut pos = 0;
                for child in c.children.iter() {
                    if let Content::Pane(pid) = child.content {
                        if pid == pane_id { break }
                    }
                    pos += 1;
                }
                pos
            },
        }
    }

    fn child_position_by_node_id(&self, node_id: NodeId) -> usize {
        match &self.content {
            Content::Pane(_) => unreachable!(),
            Content::Container(c) => {
                let mut pos = 0;
                for child in c.children.iter() {
                    if child.id == node_id { break }
                    pos += 1;
                }
                pos
            },
        }
    }
}

impl Panes {
    pub fn new(area: Rect) -> Self {
        let mut panes = BTreeMap::new();
        let focus = PaneId::default();
        let pane = Pane::new(area);
        let root_id = NodeId::default();
        let root = Node {id: root_id, parent_id: None, content: Content::Pane(focus) };
        panes.insert(focus, pane);

        Self { area, panes, focus, root, next_pane_id: focus.next(), next_node_id: root_id.next() }
    }

    pub fn resize(&mut self, new_size: Rect) {
        // recalc size for each pane, only if size has actually changed
        if new_size != self.area {
            self.area = new_size;
            // For now we just resize children equally
            self.resize_node_recursively(self.root.id, new_size);
        }
    }

    pub fn draw_borders(&mut self, buffer: &mut Buffer) {
        let mut symbols: HashMap<(u16, u16), Symbol> = HashMap::new();

        for (_, pane) in self.panes.iter() {
            pane.border_symbols(&mut symbols, self.area);
        }

        for ((x, y), symbol) in symbols {
            buffer.put_symbol(symbol.as_str(Stroke::Plain), x, y, Color::DarkGrey, Color::Reset);
        }
    }

    pub fn close(&mut self, id: PaneId) {
        debug_assert!(self.panes.len() > 1);

        let node = self.root.find_by_pane_id(id);
        _ = self.panes.remove(&self.focus);
        let parent_id = node.parent_id.unwrap();
        let parent = self.root.find(parent_id);
        let position = parent.child_position_by_pane_id(self.focus);

        match parent.content {
            Content::Pane(_) => unreachable!(),
            Content::Container(ref mut parent_container) => {
                parent_container.children.remove(position);

                if parent_container.children.len() == 1 {
                    let mut only_child = parent_container.children.remove(0);
                    only_child.parent_id = parent.parent_id;
                    // focus on:
                    // - the only child if it's a pane node
                    // - the first child of only_child that is a pane node
                    let mut focus = vec![&only_child];
                    while let Some(n) = focus.pop() {
                        match &n.content {
                            Content::Pane(pid) => self.focus = *pid,
                            Content::Container(cn) => {
                                for c in cn.children.iter().rev() {
                                    focus.push(c);
                                }
                            },
                        }
                    }
                    // Make the only child's new parent the grandparent.
                    if let Some(grandparent_id) = only_child.parent_id {
                        let grandparent = self.root.find(grandparent_id);
                        let parent_position = grandparent.child_position_by_node_id(parent_id);
                        // if the grandparent's layout is the same as the only child's layout
                        // then we compact the tree even further by eliminating the only child
                        // and only leaving its children
                        let same_layout = matches!(only_child.content, Content::Container(_)) && grandparent.layout() == only_child.layout();
                        if same_layout {
                            match (only_child.content, &mut grandparent.content) {
                                (Content::Container(only_child_container), Content::Container(ref mut grandparent_container)) => {
                                    // drop the former parent at the end of the scope
                                    _ = grandparent_container.children.remove(parent_position);
                                    for (i, mut c) in only_child_container.children.into_iter().enumerate() {
                                        c.parent_id = Some(grandparent.id);
                                        grandparent_container.children.insert(parent_position + i, c);
                                    }
                                },
                                _ => unreachable!(),
                            }
                        } else {
                            // When the layouts of the grandparent and the only child don't match
                            // we just make the only child a child of the grandparent
                            match grandparent.content {
                                Content::Pane(_) => unreachable!(),
                                Content::Container(ref mut grandparent_container) => {
                                    // Swap the parent position in the gransparent's children with the only child
                                    // At the end of this scope, the former parent will be dropped
                                    _ = std::mem::replace(&mut grandparent_container.children[parent_position], only_child);
                                },
                            }
                        }
                        // Finally resize strarting from the grandparent down
                        let area = grandparent.area();
                        self.resize_node_recursively(grandparent_id, area);
                    } else {
                        // When there is no grandparent, this means we've hit the root
                        // so we just make the only child the new root by swapping the
                        // parent with the only child
                        let cid = only_child.id;
                        _ = std::mem::replace(parent, only_child);
                        self.resize_node_recursively(cid, self.area);
                    }
                } else {
                    // focus on:
                    //  - left/up neighbour
                    //  - any other pane neighbour
                    //  - the first pane child
                    let mut focus = vec![];
                    for (i, c) in parent_container.children.iter().rev().enumerate() {
                        if i != position.saturating_sub(1) {
                            focus.push(c);
                        }
                    }
                    focus.push(&parent_container.children[position.saturating_sub(1)]);

                    while let Some(node) = focus.pop() {
                        match &node.content {
                            Content::Pane(pid) => self.focus = *pid,
                            Content::Container(cn) => {
                                for c in cn.children.iter() {
                                    focus.push(c);
                                }
                            },
                        }
                    }
                    let area = parent_container.area;
                    self.resize_node_recursively(parent_id, area);
                }
            },
        }
    }

    fn resize_node_recursively(&mut self, node_id: NodeId, area: Rect) {
        let node = self.root.find(node_id);
        let mut to_resize = vec![(node, area)];

        while let Some((node, area)) = to_resize.pop() {
            match node.content {
                Content::Container(ref mut c) => {
                    c.area = area;
                    let mut areas = match c.layout {
                        Layout::Vertical => area.split_vertically(c.children.len() as u16),
                        Layout::Horizontal => area.split_horizontally(c.children.len() as u16),
                    };
                    for child in c.children.iter_mut().rev() {
                        to_resize.push((child, areas.pop().unwrap()));
                    }
                },
                Content::Pane(pid) => { self.panes.get_mut(&pid).unwrap().area = area },
            }
        }
    }

    fn split_pane(&mut self, layout: Layout) {
        let focused = self.panes.get_mut(&self.focus).unwrap();
        let node = self.root.find_by_pane_id(self.focus);

        node.convert_to_container(self.next_node_id.advance(), layout, focused.area);
        node.insert_pane_child_at(self.next_node_id.advance(), self.next_pane_id, 1);

        self.focus = self.next_pane_id;

        let doc_id = focused.doc_id;
        self.panes.insert(self.next_pane_id.advance(), Pane {
            doc_id,
            area: Rect::default(),
            view: ScrollView::default()
        });

        let area = node.area();
        let nid = node.id;
        self.resize_node_recursively(nid, area);
    }

    pub fn split(&mut self, layout: Layout) {
        let node = self.root.find_by_pane_id(self.focus);

        // if the node is root or its parent is a different layout
        // Then we convert the current node to a container and split that
        // Otherwise, we add a new child to the parent and then
        // resize all the children
        match node.parent_id {
            Some(pid) => {
                let parent = self.root.find(pid);
                if parent.layout() == layout {
                    let focused_pane = self.panes.get(&self.focus).unwrap();

                    parent.insert_pane_child_at(
                        self.next_node_id.advance(),
                        self.next_pane_id,
                        parent.child_position_by_pane_id(self.focus) + 1
                    );

                    self.focus = self.next_pane_id;

                    self.panes.insert(self.next_pane_id.advance(), Pane {
                        doc_id: focused_pane.doc_id,
                        area: Rect::default(),
                        view: ScrollView::default()
                    });

                    let parent_id = parent.id;
                    let area = parent.area();
                    self.resize_node_recursively(parent_id, area);
                } else {
                    self.split_pane(layout)
                }
            },
            None => self.split_pane(layout),
        }
    }

    pub fn switch(&mut self, direction: Direction) {
        let focused = &self.panes[&self.focus];
        match direction {
            Direction::Up => {
                for (id, pane) in self.panes.iter() {
                    if pane.area.bottom() + 1 != focused.area.top() { continue }

                    if (pane.area.left()..=pane.area.right()).contains(&focused.view.view_cursor_position.x) {
                        self.focus = *id
                    }
                }
            },
            Direction::Down => {
                for (id, pane) in self.panes.iter() {
                    if focused.area.bottom() + 1 != pane.area.top() { continue }

                    if (pane.area.left()..=pane.area.right()).contains(&focused.view.view_cursor_position.x) {
                        self.focus = *id
                    }
                }
            },
            Direction::Left => {
                for (id, pane) in self.panes.iter() {
                    if focused.area.left() != pane.area.right() + 1 { continue }

                    if (pane.area.top()..=pane.area.bottom()).contains(&focused.view.view_cursor_position.y) {
                        self.focus = *id
                    }
                }
            },
            Direction::Right => {
                for (id, pane) in self.panes.iter() {
                    if focused.area.right() + 1 != pane.area.left() { continue }

                    if (pane.area.top()..=pane.area.bottom()).contains(&focused.view.view_cursor_position.y) {
                        self.focus = *id
                    }
                }
            },
        }
    }
}

#[derive(Debug)]
pub struct Pane {
    pub doc_id: DocumentId,
    pub area: Rect,
    pub view: ScrollView,
}

impl Pane {
    // This will always point to doc_id 1 (the default)
    // Use split to create subsequent panes
    fn new(area: Rect) -> Self {
        Self {
            area,
            doc_id: DocumentId::default(),
            view: ScrollView::default(),
        }
    }

    pub fn render(&mut self, buffer: &mut Buffer, doc: &Document, mode: &Mode, active: bool) {
        let (gutter_area, document_area) = gutter::gutter_and_document_areas(self.area, doc);

        (self.view.offset_x, self.view.offset_y) = gutter::compute_offset(document_area);

        self.render_document(document_area, buffer, doc);
        gutter::render(&self.view, gutter_area, buffer, doc, mode, active);
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

    fn border_symbols(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        if self.area.left() > area.left() {
            self.left_border_symbols(existing, area);
            if self.area.top() > area.top() {
                self.top_left_border_symbol(existing, area);
            }
            if self.area.bottom() < area.bottom() {
                self.bottom_left_border_symbol(existing, area);
            }
        }

        if self.area.right() < area.right() {
            self.right_border_symbols(existing, area);
            if self.area.top() > area.top() {
                self.top_right_border_symbol(existing, area);
            }
            if self.area.bottom() < area.bottom() {
                self.bottom_right_border_symbol(existing, area);
            }
        }

        if self.area.bottom() < area.bottom() {
            self.bottom_border_symbols(existing, area);
        }

        if self.area.top() > area.top() {
            self.top_border_symbols(existing, area)
        }
    }

    fn top_left_border_symbol(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.top() > area.top());
        debug_assert!(self.area.left() > area.left());

        find_and_intersect_with(Symbol::TopLeft, self.area.left() - 1, self.area.top() - 1, existing)
    }

    fn top_right_border_symbol(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.top() > area.top());
        debug_assert!(self.area.right() < area.right());

        find_and_intersect_with(Symbol::TopRight, self.area.right(), self.area.top() - 1, existing)
    }

    fn bottom_left_border_symbol(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.bottom() < area.bottom());
        debug_assert!(self.area.left() > area.left());

        find_and_intersect_with(Symbol::BottomLeft, self.area.left() - 1, self.area.bottom(), existing)
    }

    fn bottom_right_border_symbol(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.bottom() < area.bottom());
        debug_assert!(self.area.right() < area.right());

        find_and_intersect_with(Symbol::BottomRight, self.area.right(), self.area.bottom(), existing)
    }

    fn left_border_symbols(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.left() > area.left());

        for y in self.area.top()..self.area.bottom() {
            find_and_intersect_with(Symbol::Vertical, self.area.left() - 1, y, existing)
        }
    }

    fn right_border_symbols(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.right() < area.right());

        for y in self.area.top()..self.area.bottom() {
            find_and_intersect_with(Symbol::Vertical, self.area.right(), y, existing)
        }
    }

    fn top_border_symbols(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.top() > area.top());

        for x in self.area.left()..self.area.right() {
            find_and_intersect_with(Symbol::Horizontal, x, self.area.top() - 1, existing)
        }
    }

    fn bottom_border_symbols(&self, existing: &mut HashMap<(u16, u16), Symbol>, area: Rect) {
        debug_assert!(self.area.bottom() < area.bottom());

        for x in self.area.left()..self.area.right() {
            find_and_intersect_with(Symbol::Horizontal, x, self.area.bottom(), existing)
        }
    }
}
