use crossterm::style::Color;
use unicode_segmentation::UnicodeSegmentation;

use crate::graphemes;

use super::{style::{Modifier, Style, UnderlineStyle}, Rect};

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct Cell {
    pub symbol: String,
    pub fg: Color,
    pub bg: Color,
    pub underline_color: Color,
    pub underline_style: UnderlineStyle,
    pub modifier: Modifier,

}

impl Cell {
    fn empty() -> Self {
        Self {
            symbol: " ".to_string(),
            fg: Color::Reset,
            bg: Color::Reset,
            underline_color: Color::Reset,
            underline_style: UnderlineStyle::Reset,
            modifier: Modifier::empty(),
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
        self.set_symbol(" ")
            .set_bg(Color::Reset)
            .set_fg(Color::Reset);
        self.underline_color = Color::Reset;
        self.underline_style = UnderlineStyle::Reset;
        self.modifier = Modifier::empty();
    }

    pub fn set_style(&mut self, style: Style) -> &mut Self {
        if let Some(c) = style.fg {
            self.fg = c;
        }
        if let Some(c) = style.bg {
            self.bg = c;
        }
        if let Some(c) = style.underline_color {
            self.underline_color = c;
        }
        if let Some(style) = style.underline_style {
            self.underline_style = style;
        }

        self.modifier.insert(style.add_modifier);
        self.modifier.remove(style.sub_modifier);
        self
    }

    // pub fn style(&self) -> Style {
    //     Style::default()
    //         .fg(self.fg)
    //         .bg(self.bg)
    //         .underline_color(self.underline_color)
    //         .underline_style(self.underline_style)
    //         .add_modifier(self.modifier)
    // }
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
        self.cells.resize(size.area(), Cell::empty());
        self.size = size;
    }

    pub fn reset(&mut self) {
        for cell in &mut self.cells {
            cell.reset();
        }
    }

    pub fn diff<'a>(&'a self, other: &'a Self) -> Vec<Patch<'a>> {
        debug_assert!(self.size == other.size);

        let mut patches = vec![];

        let mut invalidated = 0;
        let mut to_skip = 0;
        for (i, (current, previous)) in other.cells.iter().zip(self.cells.iter()).enumerate() {
            if (current != previous || invalidated > 0) && to_skip == 0 {
                let x = i % self.size.width as usize;
                let y = i / self.size.width as usize;
                patches.push(Patch { x, y, cell: &other.cells[i] });
            }

            let current_width = graphemes::width(&current.symbol);
            to_skip = current_width.saturating_sub(1);

            let affected_width = current_width.max(graphemes::width(&previous.symbol));
            invalidated = affected_width.max(invalidated).saturating_sub(1);
        }

        patches
    }

    fn index(&self, x: u16, y: u16) -> usize {
        self.size.width as usize * y as usize + x as usize
    }

    pub fn get_symbol(&self, x: u16, y: u16) -> Option<&str> {
        let index = self.index(x, y);
        if let Some(cell) = self.cells.get(index) {
            return Some(&cell.symbol);
        }
        None
    }

    pub fn put_symbol(&mut self, symbol: &str, x: u16, y: u16, style: Style) {
        let index = self.index(x, y);
        if let Some(cell) = self.cells.get_mut(index) {
            cell.set_symbol(symbol).set_style(style);
        }
    }

    pub fn put_str(&mut self, str: impl AsRef<str>, x: u16, y: u16, style: Style) {
        self.put_truncated_str(str.as_ref(), x, y , self.size.right(), style);
    }

    pub fn put_truncated_str(&mut self, str: &str, mut x: u16, y: u16, right_edge: u16, style: Style) {
        let right_edge = right_edge.min(self.size.right());

        let mut graphemes = str.graphemes(true).peekable();

        while let Some(g) = graphemes.next() {
            if x >= right_edge { break }

            let index = self.index(x, y);
            let symbol = if x < right_edge.saturating_sub(1) || graphemes.peek().is_none() {
                g
            } else {
                "…"
            };

            if let Some(cell) = self.cells.get_mut(index) {
                cell.set_symbol(symbol).set_style(style);
            }

            x += graphemes::width(g) as u16;
        }
    }

    // pub fn set_style(&mut self, area: Rect, style: Style) {
    //     for y in area.top()..area.bottom() {
    //         for x in area.left()..area.right() {
    //             let index = self.index(x, y);
    //             if let Some(cell) = self.cells.get_mut(index) {
    //                 cell.set_style(style);
    //             }
    //         }
    //     }
    // }

    pub fn clear(&mut self, area: Rect) {
        for x in area.left()..area.right() {
            for y in area.top()..area.bottom() {
                let index = self.index(x, y);
                if let Some(cell) = self.cells.get_mut(index) {
                    cell.reset()
                }
            }
        }
    }

    pub fn clear_double_width_cell(&mut self, x: u16, y: u16) {
        let idx = self.index(x, y);
        if let Some(cell) = self.cells.get_mut(idx) {
            if graphemes::width(&cell.symbol) == 2 {
                cell.reset();
            }
        }
    }
}

