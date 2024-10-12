use std::borrow::Cow;

use crop::RopeSlice;

pub const NEW_LINE: char = '\n';

#[derive(Clone, Debug)]
pub struct Word<'a> {
    pub slice: RopeSlice<'a>,
    pub start: usize,
    pub end: usize,
}

impl<'a> Word<'a> {
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

