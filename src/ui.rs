pub(crate) mod buffer;
pub(crate) mod terminal;
pub(crate) mod borders;

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
}

impl From<(u16, u16)> for Rect {
    fn from(value: (u16, u16)) -> Self {
        Self { position: Position::at_origin(), width: value.0, height: value.1 }
    }
}
