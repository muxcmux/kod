pub(crate) mod buffer;
pub(crate) mod terminal;
pub(crate) mod borders;
pub(crate) mod border_box;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    pub x: u16,
    pub y: u16
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
                x: self.left(),
                y: self.top().saturating_add(height)
            },
            height: self.height.saturating_sub(height),
            ..self
        }
    }

    pub fn clip_left(self, width: u16) -> Self {
        let width = width.min(self.width);
        Self {
            position: Position {
                x: self.left().saturating_add(width),
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
                x: self.left() + (self.width.saturating_sub(width).max(1) / 2),
                y: self.top() + (self.height.saturating_sub(height).max(1) / 2),
            }
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

    /// Splits the rect vertically in two
    /// with space between the two parts
    /// returns (top_area, bottom_area)
    pub fn split_vertically(&self, space: u16) -> (Self, Self) {
        (
            self.clip_bottom((self.height + space + 1) / 2),
            self.clip_top((self.height + space) / 2)
        )
    }


    /// Splits the rect horizontally in two
    /// with space between the two parts
    /// returns (left_area, right_area)
    pub fn split_horizontally(&self, space: u16) -> (Self, Self) {
        (
            self.clip_right((self.width + space + 1) / 2),
            self.clip_left((self.width + space) / 2)
        )
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
}
