use super::{borders::{Stroke, Borders}, buffer::Buffer, style::Style, Rect};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BorderBox<'a> {
    area: Rect,
    title: Option<&'a str>,
    style: Style,
    borders: Borders,
    stroke: Stroke,
}

impl<'a> BorderBox<'a> {
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            ..Default::default()
        }
    }

    pub fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    pub fn borders(mut self, flag: Borders) -> Self {
        self.borders = flag;
        self
    }

    pub fn stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = stroke;
        self
    }

    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    pub fn inner(&self) -> Rect {
        let mut inner = self.area;
        if self.borders.intersects(Borders::LEFT) {
            inner.position.x = inner.position.x.saturating_add(1).min(inner.right());
            inner.width = inner.width.saturating_sub(1);
        }
        if self.borders.intersects(Borders::TOP) || self.title.is_some() {
            inner.position.y = inner.position.y.saturating_add(1).min(inner.bottom());
            inner.height = inner.height.saturating_sub(1);
        }
        if self.borders.intersects(Borders::RIGHT) {
            inner.width = inner.width.saturating_sub(1);
        }
        if self.borders.intersects(Borders::BOTTOM) {
            inner.height = inner.height.saturating_sub(1);
        }
        inner
    }

    pub fn render(&self, buffer: &mut Buffer) -> &Self {
        buffer.clear(self.area);

        let symbols = self.stroke.line_symbols();

        // Sides
        if self.borders.intersects(Borders::LEFT) {
            for y in self.area.top()..self.area.bottom() {
                buffer.put_symbol(symbols.vertical, self.area.left(), y, self.style);
                // workaround to fix double width cells glitch
                buffer.clear_double_width_cell(self.area.left().saturating_sub(1), y)
            }
        }
        if self.borders.intersects(Borders::TOP) {
            for x in self.area.left()..self.area.right() {
                buffer.put_symbol(symbols.horizontal, x, self.area.top(), self.style)
            }
        }
        if self.borders.intersects(Borders::RIGHT) {
            let x = self.area.right().saturating_sub(1);
            for y in self.area.top()..self.area.bottom() {
                buffer.put_symbol(symbols.vertical, x, y, self.style)
            }
        }
        if self.borders.intersects(Borders::BOTTOM) {
            let y = self.area.bottom().saturating_sub(1);
            for x in self.area.left()..self.area.right() {
                buffer.put_symbol(symbols.horizontal, x, y, self.style)
            }
        }

        // Corners
        if self.borders.contains(Borders::RIGHT | Borders::BOTTOM) {
            buffer.put_symbol(symbols.bottom_right, self.area.right().saturating_sub(1), self.area.bottom().saturating_sub(1), self.style)
        }
        if self.borders.contains(Borders::RIGHT | Borders::TOP) {
            buffer.put_symbol(symbols.top_right, self.area.right().saturating_sub(1), self.area.top(), self.style)
        }
        if self.borders.contains(Borders::LEFT | Borders::BOTTOM) {
            buffer.put_symbol(symbols.bottom_left, self.area.left(), self.area.bottom().saturating_sub(1), self.style)
        }
        if self.borders.contains(Borders::LEFT | Borders::TOP) {
            buffer.put_symbol(symbols.top_left, self.area.left(), self.area.top(), self.style)
        }

        if let Some(title) = self.title {
            let x = self.area.left() + u16::from(self.borders.intersects(Borders::LEFT));
            buffer.put_str(title, x, self.area.top(), self.style);
        }

        self
    }

    pub fn split_horizontally(&self, y: u16, buffer: &mut Buffer) {
        let y = self.area.top() + y;

        buffer.put_symbol(self.stroke.line_symbols().vertical_right, self.area.left(), y, self.style);
        buffer.put_symbol(self.stroke.line_symbols().vertical_left, self.area.right().saturating_sub(1), y, self.style);

        for i in self.area.left() + 1..self.area.right().saturating_sub(1) {
            buffer.put_symbol(self.stroke.line_symbols().horizontal, i, y, self.style);
        }
    }
}
