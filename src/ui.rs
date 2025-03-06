use crate::graphemes;

pub(crate) mod buffer;
pub(crate) mod terminal;
pub(crate) mod borders;
pub(crate) mod border_box;
pub(crate) mod text_input;
pub(crate) mod style;
pub(crate) mod theme;
pub(crate) mod scroll;
pub(crate) mod modal;

pub fn break_into_lines(string: &str, max_width: usize) -> Vec<String> {
    let width = graphemes::width(string);
    if width <= max_width {
        return vec![string.to_string()]
    }

    let mut line_width = 0;
    let mut line = String::new();
    let mut lines = Vec::with_capacity(width / max_width + 1);

    for word in string.split(' ') {
        let width = graphemes::width(word).max(1);
        if line_width + width <= max_width {
            line.push_str(word);
            line.push(' ');
            line_width += width + 1
        } else {
            lines.push(line);
            line = word.to_string();
            line.push(' ');
            line_width = width + 1;
        }
    }

    lines.push(line);

    lines
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
pub struct Position {
    pub col: u16,
    pub row: u16
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Default)]
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
                col: self.left(),
                row: self.top().saturating_add(height)
            },
            height: self.height.saturating_sub(height),
            ..self
        }
    }

    pub fn clip_left(self, width: u16) -> Self {
        let width = width.min(self.width);
        Self {
            position: Position {
                col: self.left().saturating_add(width),
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

    pub fn centered(self, width: u16, height: u16) -> Self {
        Self {
            width: self.width.min(width),
            height: self.height.min(height),
            position: Position {
                col: self.left() + (self.width.saturating_sub(width).max(1) / 2),
                row: self.top() + (self.height.saturating_sub(height).max(1) / 2),
            }
        }
    }

    pub fn left(&self) -> u16 {
        self.position.col
    }

    pub fn top(&self) -> u16 {
        self.position.row
    }

    pub fn right(&self) -> u16 {
        self.position.col + self.width
    }

    pub fn bottom(&self) -> u16 {
        self.position.row + self.height
    }

    // pub fn contains(&self, position: &Position) -> bool {
    //     position.col < self.right() &&
    //         position.col >= self.left() &&
    //         position.row < self.bottom() &&
    //         position.row >= self.top()
    // }

    /// Splits the rect vertically into N parts
    /// with a single row/col space between each part
    pub fn split_vertically(&self, n: u16) -> Vec<Rect> {
        debug_assert!(n > 0);

        let height = self.height.saturating_sub(n.saturating_sub(1)) / n;
        let rem = self.height.saturating_sub(n.saturating_sub(1)) % n;

        let mut heights = Vec::with_capacity(n as usize);

        for i in 1..=n {
            heights.push(height + 1 - i.saturating_sub(rem).min(1));
        }

        let mut y = self.top();

        heights.into_iter().map(|height| {
            let area = Rect {
                position: Position {
                    row: y,
                    col: self.left(),
                },
                height,
                ..*self
            };
            y += height + 1;
            area
        }).collect()
    }

    /// Splits the rect horizontally into N parts
    /// with a single row/col space between each part
    pub fn split_horizontally(&self, n: u16) -> Vec<Rect> {
        debug_assert!(n > 0);

        let width = self.width.saturating_sub(n.saturating_sub(1)) / n;
        let rem = self.width.saturating_sub(n.saturating_sub(1)) % n;

        let mut widths = Vec::with_capacity(n as usize);

        for i in 1..=n {
            widths.push(width + 1 - i.saturating_sub(rem).min(1));
        }

        let mut x = self.left();

        widths.into_iter().map(|width| {
            let area = Rect {
                position: Position {
                    col: x,
                    row: self.top(),
                },
                width,
                ..*self
            };
            x += width + 1;
            area
        }).collect()
    }
}

impl From<(u16, u16)> for Rect {
    fn from((width, height): (u16, u16)) -> Self {
        Self { width,  height, ..Default::default() }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_area() {
        assert_eq!(Rect::from((10, 10)).area(), 100);
    }

    #[test]
    fn test_clip_bottom() {
        let rect = Rect::from((10, 10));
        let clipped = rect.clip_bottom(1);
        assert_eq!(clipped.top(), 0);
        assert_eq!(clipped.left(), 0);
        assert_eq!(clipped.right(), 10);
        assert_eq!(clipped.bottom(), 9);
        assert_eq!(clipped.height, 9);
        assert_eq!(clipped.width, 10);
    }

    #[test]
    fn test_clip_top() {
        let rect = Rect::from((10, 10));
        let clipped = rect.clip_top(1);
        assert_eq!(clipped.top(), 1);
        assert_eq!(clipped.left(), 0);
        assert_eq!(clipped.right(), 10);
        assert_eq!(clipped.bottom(), 10);
        assert_eq!(clipped.height, 9);
        assert_eq!(clipped.width, 10);
    }

    #[test]
    fn test_clip_left() {
        let rect = Rect::from((10, 10));
        let clipped = rect.clip_left(1);
        assert_eq!(clipped.top(), 0);
        assert_eq!(clipped.left(), 1);
        assert_eq!(clipped.right(), 10);
        assert_eq!(clipped.bottom(), 10);
        assert_eq!(clipped.height, 10);
        assert_eq!(clipped.width, 9);
    }

    #[test]
    fn test_clip_right() {
        let rect = Rect::from((10, 10));
        let clipped = rect.clip_right(1);
        assert_eq!(clipped.top(), 0);
        assert_eq!(clipped.left(), 0);
        assert_eq!(clipped.right(), 9);
        assert_eq!(clipped.bottom(), 10);
        assert_eq!(clipped.height, 10);
        assert_eq!(clipped.width, 9);
    }

    #[test]
    fn test_centered() {
        let rect = Rect::from((100, 100));
        let centered = rect.centered(10, 10);
        assert_eq!(centered.top(), 45);
        assert_eq!(centered.left(), 45);
        assert_eq!(centered.right(), 55);
        assert_eq!(centered.bottom(), 55);
    }

    #[test]
    fn test_split_vertically() {
        let rect = Rect::from((10, 10));
        let mut splits = rect.split_vertically(3);
        println!("{:#?}", splits);
        assert_eq!(splits.pop(), Some(Rect {
            position: Position { col: 0, row: 8 },
            width: 10,
            height: 2,
        }));
        assert_eq!(splits.pop(), Some(Rect {
            position: Position { col: 0, row: 4 },
            width: 10,
            height: 3,
        }));
        assert_eq!(splits.pop(), Some(Rect {
            position: Position { col: 0, row: 0 },
            width: 10,
            height: 3,
        }));
        assert_eq!(splits.pop(), None);
    }

    #[test]
    fn test_split_horizontally() {
        let rect = Rect::from((11, 10));
        let mut splits = rect.split_horizontally(2);
        println!("{:#?}", splits);
        assert_eq!(splits.pop(), Some(Rect {
            position: Position { col: 6, row: 0 },
            width: 5,
            height: 10,
        }));
        assert_eq!(splits.pop(), Some(Rect {
            position: Position { col: 0, row: 0 },
            width: 5,
            height: 10,
        }));
        assert_eq!(splits.pop(), None);
    }
}
