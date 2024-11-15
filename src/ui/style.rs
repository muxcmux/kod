use std::str::FromStr;

// Pretty much a helix copy
use bitflags::bitflags;
use crossterm::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnderlineStyle {
    Reset,
    Line,
    Curl,
    Dotted,
    Dashed,
    DoubleLine,
}

impl From<UnderlineStyle> for crossterm::style::Attribute {
    fn from(style: UnderlineStyle) -> Self {
        match style {
            UnderlineStyle::Line       => crossterm::style::Attribute::Underlined,
            UnderlineStyle::Curl       => crossterm::style::Attribute::Undercurled,
            UnderlineStyle::Dotted     => crossterm::style::Attribute::Underdotted,
            UnderlineStyle::Dashed     => crossterm::style::Attribute::Underdashed,
            UnderlineStyle::DoubleLine => crossterm::style::Attribute::DoubleUnderlined,
            UnderlineStyle::Reset      => crossterm::style::Attribute::NoUnderline,
        }
    }
}

impl FromStr for UnderlineStyle {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "reset" => Ok(Self::Reset),
            "line" => Ok(Self::Line),
            "curl" => Ok(Self::Curl),
            "dot" => Ok(Self::Dotted),
            "dash" => Ok(Self::Dashed),
            "double" => Ok(Self::DoubleLine),
            _ => Err("Invalid underline style")
        }
    }
}

bitflags! {
    #[derive(PartialEq, Eq, Debug, Clone, Copy)]
    pub struct Modifier: u8 {
        const BOLD              = 0b0000_0001;
        const DIM               = 0b0000_0010;
        const ITALIC            = 0b0000_0100;
        const SLOW_BLINK        = 0b0000_1000;
        const RAPID_BLINK       = 0b0001_0000;
        const REVERSED          = 0b0010_0000;
        const HIDDEN            = 0b0100_0000;
        const CROSSED_OUT       = 0b1000_0000;
    }
}

impl FromStr for Modifier {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bold" => Ok(Self::BOLD),
            "dim" => Ok(Self::DIM),
            "italic" => Ok(Self::ITALIC),
            "blink" => Ok(Self::SLOW_BLINK),
            "bblink" => Ok(Self::RAPID_BLINK),
            "rev" => Ok(Self::REVERSED),
            "hidden" => Ok(Self::HIDDEN),
            "strike" => Ok(Self::CROSSED_OUT),
            _ => Err("Invalid mod")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub underline_color: Option<Color>,
    pub underline_style: Option<UnderlineStyle>,
    pub add_modifier: Modifier,
    pub sub_modifier: Modifier,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fg: None,
            bg: None,
            underline_color: None,
            underline_style: None,
            add_modifier: Modifier::empty(),
            sub_modifier: Modifier::empty(),
        }
    }
}

impl Style {
    pub const fn reset() -> Self {
        Self {
            fg: Some(Color::Reset),
            bg: Some(Color::Reset),
            underline_color: None,
            underline_style: None,
            add_modifier: Modifier::empty(),
            sub_modifier: Modifier::all(),
        }
    }

    pub const fn fg(mut self, color: Color) -> Style {
        self.fg = Some(color);
        self
    }

    pub const fn bg(mut self, color: Color) -> Style {
        self.bg = Some(color);
        self
    }

    pub const fn underline_color(mut self, color: Color) -> Style {
        self.underline_color = Some(color);
        self
    }

    pub const fn underline_style(mut self, style: UnderlineStyle) -> Style {
        self.underline_style = Some(style);
        self
    }

    pub fn add_modifier(mut self, modifier: Modifier) -> Style {
        self.sub_modifier.remove(modifier);
        self.add_modifier.insert(modifier);
        self
    }

    pub fn remove_modifier(mut self, modifier: Modifier) -> Style {
        self.add_modifier.remove(modifier);
        self.sub_modifier.insert(modifier);
        self
    }

    pub fn patch(mut self, other: Style) -> Style {
        self.fg = other.fg.or(self.fg);
        self.bg = other.bg.or(self.bg);
        self.underline_color = other.underline_color.or(self.underline_color);
        self.underline_style = other.underline_style.or(self.underline_style);

        self.add_modifier.remove(other.sub_modifier);
        self.add_modifier.insert(other.add_modifier);
        self.sub_modifier.remove(other.add_modifier);
        self.sub_modifier.insert(other.sub_modifier);

        self
    }
}
