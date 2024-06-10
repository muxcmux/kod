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
}

impl From<(u16, u16)> for Rect {
    fn from((width, height): (u16, u16)) -> Self {
        Self { width,  height, ..Default::default() }
    }
}
