use std::borrow::Cow;

use crop::{Rope, RopeSlice};

pub const NEW_LINE: char = '\n';
pub const NEW_LINE_STR: &str = "\n";
pub const NEW_LINE_STR_WIN: &str = "\r\n";

pub fn width(s: &str) -> usize {
    unicode_display_width::width(s) as usize
}

pub fn line_width(rope: &Rope, line: usize) -> usize {
    rope.line(line).graphemes().map(|g| width(&g)).sum()
}

pub fn words_of_line(rope: &Rope, y: usize, exclude_blank_words: bool) -> Vec<Word<'_>> {
    let line = rope.line(y);
    let mut offset = 0;
    let mut word_start_byte = offset;
    let mut words = vec![];
    let mut col = 0;
    let mut word = Word { start: col, end: col, slice: line.byte_slice(..) };
    let mut iter = line.graphemes().peekable();

    while let Some(g) = iter.next() {
        let width = width(&g);
        let size = g.len();
        let this_cat = GraphemeCategory::from(&g);
        match iter.peek() {
            Some(next) => {
                let next_cat = GraphemeCategory::from(next);
                if this_cat != next_cat {
                    // that's the end of the current word
                    // and the index has to fall on the first
                    // column of a grapheme
                    word.end = col;
                    word.slice = line.byte_slice(word_start_byte..offset + size);
                    // push it to the list of words
                    words.push(word.clone());
                    // start the next word
                    word.start = col + width;
                    word_start_byte = offset + size;
                }
            }
            None => {
                // this is the end of the last word
                // and the index has to fall on the first
                // column of a grapheme
                word.end = col;
                word.slice = line.byte_slice(word_start_byte..offset + size);
                words.push(word);
                break;
            }
        }

        col += width;
        offset += size;
    }

    if exclude_blank_words {
        words.into_iter().filter(|w| !w.is_blank()).collect()
    } else {
        words
    }
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

