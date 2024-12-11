use std::borrow::Cow;

use crop::RopeSlice;

pub const NEW_LINE: char = '\n';
pub const NEW_LINE_STR: &str = "\n";
pub const NEW_LINE_STR_WIN: &str = "\r\n";

pub fn width(s: &str) -> usize {
    unicode_display_width::width(s) as usize
}

#[derive(Clone, Debug)]
pub struct Word<'a> {
    pub slice: RopeSlice<'a>,
    pub start: usize,
    pub end: usize,
}

impl Word<'_> {
    pub fn is_blank(&self) -> bool {
        self.slice.chars().all(|c| c.is_whitespace())
    }
}

#[derive(PartialEq)]
pub enum GraphemeCategory {
    Whitespace,
    Word,
    Punctuation,
    Other,
}

pub fn grapheme_is_line_ending(grapheme: &str) -> bool {
    matches!(grapheme, NEW_LINE_STR | NEW_LINE_STR_WIN)
}

impl From<&Cow<'_, str>> for GraphemeCategory {
    fn from(g: &Cow<'_, str>) -> Self {
        use unicode_general_category::{get_general_category, GeneralCategory::*};
        match g.chars().next() {
            Some(c) => match c {
                ws if ws.is_whitespace() => Self::Whitespace,
                a if a.is_alphanumeric() => Self::Word,
                '-' | '_' => Self::Word,
                _ => match get_general_category(c) {
                    OtherPunctuation
                        | OpenPunctuation
                        | ClosePunctuation
                        | InitialPunctuation
                        | FinalPunctuation
                        | ConnectorPunctuation
                        | DashPunctuation
                        | MathSymbol
                        | CurrencySymbol
                        | ModifierSymbol => Self::Punctuation,
                    _ => Self::Other
                }
            },
            None => Self::Other
        }
    }
}

