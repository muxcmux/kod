use std::ops::Range;

use crossterm::style::Color;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub x: u16,
    pub y: u16
}

impl Position {
    pub fn at_origin() -> Self {
        Position { x: 0, y: 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub position: Position,
    pub width: u16,
    pub height: u16
}

impl Rect {
    pub fn area(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn clip_bottom(self, height: u16) -> Self {
        Self {
            height: self.height.saturating_sub(height),
            ..self
        }
    }

    pub fn clip_top(self, height: u16) -> Self {
        let height = height.min(self.height);
        Self {
            position: Position {
                x: self.position.x,
                y: self.position.y.saturating_add(height)
            },
            height: self.height.saturating_sub(height),
            ..self
        }
    }

    pub fn clip_left(self, width: u16) -> Self {
        let width = width.min(self.width);
        Self {
            position: Position {
                x: self.position.x.saturating_add(width),
                ..self.position
            },
            width: self.width.saturating_sub(width),
            ..self
        }
    }

    pub fn clip_right(self, width: u16) -> Self {
        Self {
            width: self.width.saturating_sub(width),
            ..self
        }
    }

    pub fn left(&self) -> u16 {
        self.position.x
    }

    pub fn top(&self) -> u16 {
        self.position.y
    }

    pub fn right(&self) -> u16 {
        self.position.x + self.width
    }

    pub fn bottom(&self) -> u16 {
        self.position.y + self.height
    }

    pub fn v_range(&self) -> Range<u16> {
        self.top()..self.bottom() + 1
    }

    pub fn h_range(&self) -> Range<u16> {
        self.left()..self.right()
    }
}

impl From<(u16, u16)> for Rect {
    fn from(value: (u16, u16)) -> Self {
        Self { position: Position::at_origin(), width: value.0, height: value.1 }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub struct Cell {
    pub symbol: String,
    pub fg: Color,
    pub bg: Color,
}

impl Cell {
    fn empty() -> Self {
        Self {
            symbol: " ".to_string(),
            fg: Color::Reset,
            bg: Color::Reset,
        }
    }

    fn set_symbol(&mut self, symbol: &str) -> &mut Self {
        self.symbol.clear();
        self.symbol.push_str(symbol);
        self
    }

    fn set_fg(&mut self, fg: Color) -> &mut Self {
        self.fg = fg;
        self
    }

    fn set_bg(&mut self, bg: Color) -> &mut Self {
        self.bg = bg;
        self
    }

    fn reset(&mut self) {
        self.set_symbol(" ");
    }
}


#[derive(Debug)]
pub struct Patch<'a> {
    pub cell: &'a Cell,
    pub x: usize,
    pub y: usize,
}

#[derive(Clone, Debug)]
pub struct Buffer {
    cells: Vec<Cell>,
    size: Rect,
}

impl Buffer {
    pub fn new(size: Rect) -> Self {
        let cells = vec![Cell::empty(); size.area()];
        Self {
            size,
            cells,
        }
    }

    pub fn resize(&mut self, size: Rect) {
        let length = size.area();
        self.cells.resize(length, Cell::empty());
        self.size = size;
    }

    pub fn reset(&mut self) {
        for cell in &mut self.cells {
            cell.reset();
        }
    }

    pub fn diff<'a>(&'a self, other: &'a Self) -> Vec<Patch> {
        assert!(self.size == other.size);

        let mut patches = vec![];

        let mut invalidated = 0;
        let mut to_skip = 0;
        for (i, (current, previous)) in other.cells.iter().zip(self.cells.iter()).enumerate() {
            if (current != previous || invalidated > 0) && to_skip == 0 {
                let x = i % self.size.width as usize;
                let y = i / self.size.width as usize;
                patches.push(Patch { x, y, cell: &other.cells[i] });
            }

            let current_width = unicode_display_width::width(&current.symbol);
            to_skip = current_width.saturating_sub(1);

            let affected_width = current_width.max(unicode_display_width::width(&previous.symbol));
            invalidated = affected_width.max(invalidated).saturating_sub(1);
        }

        patches
    }

    fn index(&self, x: u16, y: u16) -> usize {
        self.size.width as usize * y as usize + x as usize
    }

    pub fn put_symbol(&mut self, symbol: String, x: u16, y: u16, fg: Color, bg: Color) {
        let index = self.index(x, y);
        if let Some(cell) = self.cells.get_mut(index) {
            cell.set_symbol(&symbol)
                .set_fg(fg)
                .set_bg(bg);
        }
    }

    pub fn put_string(&mut self, string: String, x: u16, y: u16, fg: Color, bg: Color) {
        let start = self.index(x, y);

        for (offset, g) in string.graphemes(true).enumerate() {
            if start + offset > self.cells.len() {
                break;
            }
            if let Some(cell) = self.cells.get_mut(start + offset) {
                cell.set_symbol(g)
                    .set_fg(fg)
                    .set_bg(bg);
            }
        }
    }
}

