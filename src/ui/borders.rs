use core::fmt;
use std::fmt::Write;

use bitflags::bitflags;

pub const VERTICAL: &str = "│";
pub const DOUBLE_VERTICAL: &str = "║";
pub const THICK_VERTICAL: &str = "┃";

pub const HORIZONTAL: &str = "─";
pub const DOUBLE_HORIZONTAL: &str = "═";
pub const THICK_HORIZONTAL: &str = "━";

pub const TOP_RIGHT: &str = "┐";
pub const ROUNDED_TOP_RIGHT: &str = "╮";
pub const DOUBLE_TOP_RIGHT: &str = "╗";
pub const THICK_TOP_RIGHT: &str = "┓";

pub const TOP_LEFT: &str = "┌";
pub const ROUNDED_TOP_LEFT: &str = "╭";
pub const DOUBLE_TOP_LEFT: &str = "╔";
pub const THICK_TOP_LEFT: &str = "┏";

pub const BOTTOM_RIGHT: &str = "┘";
pub const ROUNDED_BOTTOM_RIGHT: &str = "╯";
pub const DOUBLE_BOTTOM_RIGHT: &str = "╝";
pub const THICK_BOTTOM_RIGHT: &str = "┛";

pub const BOTTOM_LEFT: &str = "└";
pub const ROUNDED_BOTTOM_LEFT: &str = "╰";
pub const DOUBLE_BOTTOM_LEFT: &str = "╚";
pub const THICK_BOTTOM_LEFT: &str = "┗";

pub const VERTICAL_LEFT: &str = "┤";
pub const DOUBLE_VERTICAL_LEFT: &str = "╣";
pub const THICK_VERTICAL_LEFT: &str = "┫";

pub const VERTICAL_RIGHT: &str = "├";
pub const DOUBLE_VERTICAL_RIGHT: &str = "╠";
pub const THICK_VERTICAL_RIGHT: &str = "┣";

pub const HORIZONTAL_DOWN: &str = "┬";
pub const DOUBLE_HORIZONTAL_DOWN: &str = "╦";
pub const THICK_HORIZONTAL_DOWN: &str = "┳";

pub const HORIZONTAL_UP: &str = "┴";
pub const DOUBLE_HORIZONTAL_UP: &str = "╩";
pub const THICK_HORIZONTAL_UP: &str = "┻";

pub const CROSS: &str = "┼";
pub const DOUBLE_CROSS: &str = "╬";
pub const THICK_CROSS: &str = "╋";

#[derive(Debug, Clone)]
pub struct Set {
    pub vertical: &'static str,
    pub horizontal: &'static str,
    pub top_right: &'static str,
    pub top_left: &'static str,
    pub bottom_right: &'static str,
    pub bottom_left: &'static str,
    pub vertical_left: &'static str,
    pub vertical_right: &'static str,
    pub horizontal_down: &'static str,
    pub horizontal_up: &'static str,
    pub cross: &'static str,
}

pub const NORMAL: Set = Set {
    vertical: VERTICAL,
    horizontal: HORIZONTAL,
    top_right: TOP_RIGHT,
    top_left: TOP_LEFT,
    bottom_right: BOTTOM_RIGHT,
    bottom_left: BOTTOM_LEFT,
    vertical_left: VERTICAL_LEFT,
    vertical_right: VERTICAL_RIGHT,
    horizontal_down: HORIZONTAL_DOWN,
    horizontal_up: HORIZONTAL_UP,
    cross: CROSS,
};

pub const ROUNDED: Set = Set {
    top_right: ROUNDED_TOP_RIGHT,
    top_left: ROUNDED_TOP_LEFT,
    bottom_right: ROUNDED_BOTTOM_RIGHT,
    bottom_left: ROUNDED_BOTTOM_LEFT,
    ..NORMAL
};

pub const DOUBLE: Set = Set {
    vertical: DOUBLE_VERTICAL,
    horizontal: DOUBLE_HORIZONTAL,
    top_right: DOUBLE_TOP_RIGHT,
    top_left: DOUBLE_TOP_LEFT,
    bottom_right: DOUBLE_BOTTOM_RIGHT,
    bottom_left: DOUBLE_BOTTOM_LEFT,
    vertical_left: DOUBLE_VERTICAL_LEFT,
    vertical_right: DOUBLE_VERTICAL_RIGHT,
    horizontal_down: DOUBLE_HORIZONTAL_DOWN,
    horizontal_up: DOUBLE_HORIZONTAL_UP,
    cross: DOUBLE_CROSS,
};

pub const THICK: Set = Set {
    vertical: THICK_VERTICAL,
    horizontal: THICK_HORIZONTAL,
    top_right: THICK_TOP_RIGHT,
    top_left: THICK_TOP_LEFT,
    bottom_right: THICK_BOTTOM_RIGHT,
    bottom_left: THICK_BOTTOM_LEFT,
    vertical_left: THICK_VERTICAL_LEFT,
    vertical_right: THICK_VERTICAL_RIGHT,
    horizontal_down: THICK_HORIZONTAL_DOWN,
    horizontal_up: THICK_HORIZONTAL_UP,
    cross: THICK_CROSS,
};

bitflags! {
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
    pub struct Borders: u8 {
        const TOP = 0b0000_0001;
        const RIGHT = 0b0000_0010;
        const BOTTOM = 0b000_0100;
        const LEFT = 0b0000_1000;
        const ALL = Self::TOP.bits() | Self::RIGHT.bits() | Self::BOTTOM.bits() | Self::LEFT.bits();
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    #[default]
    Plain,
    Rounded,
    Double,
    Thick,
}

impl Stroke {
    pub fn line_symbols(&self) -> Set {
        match self {
            Self::Plain => NORMAL,
            Self::Rounded => ROUNDED,
            Self::Double => DOUBLE,
            Self::Thick => THICK,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Symbol {
    Vertical,
    Horizontal,
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
    VerticalLeft,
    VerticalRight,
    HorizontalDown,
    HorizontalUp,
    Cross,
}

impl Symbol {
    pub fn as_str(&self, stroke: Stroke) -> &str {
        let symbols = stroke.line_symbols();
        match self {
            Self::Vertical       => symbols.vertical,
            Self::Horizontal     => symbols.horizontal,
            Self::TopRight       => symbols.top_right,
            Self::TopLeft        => symbols.top_left,
            Self::BottomRight    => symbols.bottom_right,
            Self::BottomLeft     => symbols.bottom_left,
            Self::VerticalLeft   => symbols.vertical_left,
            Self::VerticalRight  => symbols.vertical_right,
            Self::HorizontalDown => symbols.horizontal_down,
            Self::HorizontalUp   => symbols.horizontal_up,
            Self::Cross          => symbols.cross,
        }
    }

    pub fn intersect(&self, other: Self) -> Self {
        use Symbol::*;

        match self {
            Vertical => match other {
                Vertical       => Vertical,
                Horizontal     => Cross,
                TopRight       => VerticalLeft,
                TopLeft        => VerticalRight,
                BottomRight    => VerticalLeft,
                BottomLeft     => VerticalRight,
                VerticalLeft   => VerticalLeft,
                VerticalRight  => VerticalRight,
                HorizontalDown => Cross,
                HorizontalUp   => Cross,
                Cross          => Cross,
            },
            Horizontal => match other {
                Vertical       => Cross,
                Horizontal     => Horizontal,
                TopRight       => HorizontalDown,
                TopLeft        => HorizontalDown,
                BottomRight    => HorizontalUp,
                BottomLeft     => HorizontalUp,
                VerticalLeft   => Cross,
                VerticalRight  => Cross,
                HorizontalDown => HorizontalDown,
                HorizontalUp   => HorizontalUp,
                Cross          => Cross,
            },
            TopRight => match other {
                Vertical       => VerticalLeft,
                Horizontal     => HorizontalDown,
                TopRight       => TopRight,
                TopLeft        => HorizontalDown,
                BottomRight    => VerticalLeft,
                BottomLeft     => Cross,
                VerticalLeft   => VerticalLeft,
                VerticalRight  => Cross,
                HorizontalDown => HorizontalDown,
                HorizontalUp   => Cross,
                Cross          => Cross,
            },
            TopLeft => match other {
                Vertical       => VerticalRight,
                Horizontal     => HorizontalDown,
                TopRight       => HorizontalDown,
                TopLeft        => TopLeft,
                BottomRight    => Cross,
                BottomLeft     => VerticalRight,
                VerticalLeft   => Cross,
                VerticalRight  => VerticalRight,
                HorizontalDown => HorizontalDown,
                HorizontalUp   => Cross,
                Cross          => Cross,
            },
            BottomRight => match other {
                Vertical       => VerticalLeft,
                Horizontal     => HorizontalUp,
                TopRight       => VerticalLeft,
                TopLeft        => Cross,
                BottomRight    => BottomRight,
                BottomLeft     => HorizontalUp,
                VerticalLeft   => VerticalLeft,
                VerticalRight  => Cross,
                HorizontalDown => Cross,
                HorizontalUp   => HorizontalUp,
                Cross          => Cross,
            },
            BottomLeft => match other {
                Vertical       => VerticalRight,
                Horizontal     => HorizontalUp,
                TopRight       => Cross,
                TopLeft        => VerticalRight,
                BottomRight    => HorizontalUp,
                BottomLeft     => BottomLeft,
                VerticalLeft   => Cross,
                VerticalRight  => VerticalRight,
                HorizontalDown => Cross,
                HorizontalUp   => HorizontalUp,
                Cross          => Cross,
            },
            VerticalLeft => match other {
                Vertical       => VerticalLeft,
                Horizontal     => Cross,
                TopRight       => VerticalLeft,
                TopLeft        => Cross,
                BottomRight    => VerticalLeft,
                BottomLeft     => Cross,
                VerticalLeft   => VerticalLeft,
                VerticalRight  => Cross,
                HorizontalDown => Cross,
                HorizontalUp   => Cross,
                Cross          => Cross,
            },
            VerticalRight => match other {
                Vertical       => VerticalRight,
                Horizontal     => Cross,
                TopRight       => Cross,
                TopLeft        => VerticalRight,
                BottomRight    => Cross,
                BottomLeft     => VerticalRight,
                VerticalLeft   => Cross,
                VerticalRight  => VerticalRight,
                HorizontalDown => Cross,
                HorizontalUp   => Cross,
                Cross          => Cross,
            },
            HorizontalDown => match other {
                Vertical       => Cross,
                Horizontal     => HorizontalDown,
                TopRight       => HorizontalDown,
                TopLeft        => HorizontalDown,
                BottomRight    => Cross,
                BottomLeft     => Cross,
                VerticalLeft   => Cross,
                VerticalRight  => Cross,
                HorizontalDown => HorizontalDown,
                HorizontalUp   => Cross,
                Cross          => Cross,
            },
            HorizontalUp => match other {
                Vertical       => Cross,
                Horizontal     => HorizontalUp,
                TopRight       => Cross,
                TopLeft        => Cross,
                BottomRight    => HorizontalUp,
                BottomLeft     => HorizontalUp,
                VerticalLeft   => Cross,
                VerticalRight  => Cross,
                HorizontalDown => Cross,
                HorizontalUp   => HorizontalUp,
                Cross          => Cross,
            },
            Cross => Cross
        }
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str(Stroke::Thick))
    }
}
