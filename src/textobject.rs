use std::iter::{Peekable, Rev};

use crop::{iter::Graphemes, Rope, RopeSlice};
use crossterm::event::KeyCode;

use crate::{graphemes::{width, GraphemeCategory}, selection::Selection};

// Need to expand this to account for starting and ending row as well
pub struct Range {
    pub start: usize,
    pub end: usize,
    pub start_byte: usize,
    pub end_byte: usize,
}

impl Range {
    pub fn slice<'a>(&self, slice: RopeSlice<'a>) -> RopeSlice<'a> {
        slice.byte_slice(self.start_byte..self.end_byte)
    }

    pub fn is_blank(&self, slice: RopeSlice<'_>) -> bool {
        self.slice(slice).chars().all(|c| c.is_whitespace())
    }

    pub fn contains(&self, col: &usize) -> bool {
        (self.start..=self.end).contains(col)
    }
}

pub enum TextObjectKind {
    Word,
    LongWord,
}

impl TryFrom<KeyCode> for TextObjectKind {
    type Error = String;

    fn try_from(value: KeyCode) -> Result<Self, Self::Error> {
        match value {
            KeyCode::Char(c) => match c {
                'w' => Ok(Self::Word),
                'W' => Ok(Self::LongWord),
                _ => Err(format!("'{c}' does not map to a valid TextObjectKind"))
            },
            _ => Err(format!("{value} does not map to a valid TextObjectKind")),
        }
    }
}

impl TextObjectKind {
    pub fn inside(&self, rope: &Rope, selection: &Selection) -> Range {
        match self {
            Self::Word => {
                let mut words = Words::new(rope.line(selection.head.y));
                words.find(|w| w.contains(&selection.head.x)).unwrap()
            },
            Self::LongWord => {
                let mut words = LongWords::new(rope.line(selection.head.y));
                words.find(|w| w.contains(&selection.head.x)).unwrap()
            }
        }
    }
}

// ---- Iterators ----

pub struct Words<'a> {
    offset: usize,
    col: usize,
    graphemes: Peekable<Graphemes<'a>>,
}

pub struct WordsBackwards<'a> {
    offset: usize,
    col: usize,
    graphemes: Peekable<Rev<Graphemes<'a>>>,
}

pub struct LongWords<'a> {
    offset: usize,
    col: usize,
    graphemes: Peekable<Graphemes<'a>>,
}

pub struct LongWordsBackwards<'a> {
    offset: usize,
    col: usize,
    graphemes: Peekable<Rev<Graphemes<'a>>>,
}

impl<'a> Words<'a> {
    pub fn new(slice: RopeSlice<'a>) -> Self {
        Self {
            col: 0,
            offset: 0,
            graphemes: slice.graphemes().peekable(),
        }
    }
}

impl<'a> WordsBackwards<'a> {
    pub fn new(slice: RopeSlice<'a>) -> Self {
        let col = slice.graphemes().map(|g| width(&g)).sum::<usize>().saturating_sub(1);

        Self {
            col,
            offset: slice.byte_len(),
            graphemes: slice.graphemes().rev().peekable(),
        }
    }
}

impl<'a> LongWords<'a> {
    pub fn new(slice: RopeSlice<'a>) -> Self {
        Self {
            col: 0,
            offset: 0,
            graphemes: slice.graphemes().peekable(),
        }
    }
}

impl<'a> LongWordsBackwards<'a> {
    pub fn new(slice: RopeSlice<'a>) -> Self {
        let col = slice.graphemes().map(|g| width(&g)).sum::<usize>().saturating_sub(1);

        Self {
            col,
            offset: slice.byte_len(),
            graphemes: slice.graphemes().rev().peekable(),
        }
    }
}

impl Iterator for Words<'_> {
    type Item = Range;

    fn next(&mut self) -> Option<Self::Item> {
        let mut col = self.col;
        let mut offset = self.offset;

        while let Some(g) = self.graphemes.next() {
            let width = width(&g);
            let size = g.len();
            let this_cat = GraphemeCategory::from(&g);
            match self.graphemes.peek() {
                Some(next) => {
                    let next_cat = GraphemeCategory::from(next);
                    if this_cat != next_cat {
                        // that's the end of the current word
                        // and the index has to fall on the first
                        // column of a grapheme
                        let end_byte = offset + size;

                        let word = Range {
                            start_byte: self.offset,
                            end_byte,
                            start: self.col,
                            end: col,
                        };

                        self.col = col + width;
                        self.offset = end_byte;

                        return Some(word)
                    }
                }
                None => {
                    // this is the end of the last word
                    // and the index has to fall on the first
                    // column of a grapheme
                    let end_byte = offset + size;
                    return Some(Range {
                        start_byte: self.offset,
                        end_byte,
                        start: self.col,
                        end: col,
                    })
                }
            }

            col += width;
            offset += size;
        }

        None
    }
}

impl Iterator for WordsBackwards<'_> {
    type Item = Range;

    fn next(&mut self) -> Option<Self::Item> {
        let mut col = self.col;
        let mut offset = self.offset;

        while let Some(g) = self.graphemes.next() {
            let width = width(&g);
            let size = g.len();
            let this_cat = GraphemeCategory::from(&g);
            match self.graphemes.peek() {
                Some(next) => {
                    let next_cat = GraphemeCategory::from(next);
                    if this_cat != next_cat {
                        // that's the start of the current word
                        // and the index has to fall on the first
                        // column of a grapheme
                        let start_byte = offset.saturating_sub(size);

                        // start and end are reversed
                        let word = Range {
                            end_byte: self.offset,
                            start_byte,
                            end: self.col.saturating_sub(width - 1),
                            start: col.saturating_sub(width - 1),
                        };

                        self.col = col.saturating_sub(width);
                        self.offset = start_byte;

                        return Some(word)
                    }
                }
                None => {
                    // this is the start of the first word
                    // and the index has to fall on the first
                    // column of a grapheme
                    let start_byte = offset.saturating_sub(size);
                    return Some(Range {
                        end_byte: self.offset,
                        start_byte,
                        end: self.col,
                        start: col,
                    })
                }
            }

            col = col.saturating_sub(width);
            offset = offset.saturating_sub(size);
        }

        None
    }
}

impl Iterator for LongWords<'_> {
    type Item = Range;

    fn next(&mut self) -> Option<Self::Item> {
        let mut col = self.col;
        let mut offset = self.offset;

        while let Some(g) = self.graphemes.next() {
            let width = width(&g);
            let size = g.len();
            let this_cat = GraphemeCategory::from(&g);
            match self.graphemes.peek() {
                Some(next) => {
                    let next_cat = GraphemeCategory::from(next);
                    if (this_cat != GraphemeCategory::Whitespace && next_cat == GraphemeCategory::Whitespace) ||
                        (this_cat == GraphemeCategory::Whitespace && next_cat != GraphemeCategory::Whitespace) {
                        // that's the end of the current word
                        // and the index has to fall on the first
                        // column of a grapheme
                        let end_byte = offset + size;

                        let word = Range {
                            start_byte: self.offset,
                            end_byte,
                            start: self.col,
                            end: col,
                        };

                        self.col = col + width;
                        self.offset = end_byte;

                        return Some(word)
                    }
                }
                None => {
                    // this is the end of the last word
                    // and the index has to fall on the first
                    // column of a grapheme
                    let end_byte = offset + size;
                    return Some(Range {
                        start_byte: self.offset,
                        end_byte,
                        start: self.col,
                        end: col,
                    })
                }
            }

            col += width;
            offset += size;
        }

        None
    }
}

impl Iterator for LongWordsBackwards<'_> {
    type Item = Range;

    fn next(&mut self) -> Option<Self::Item> {
        let mut col = self.col;
        let mut offset = self.offset;

        while let Some(g) = self.graphemes.next() {
            let width = width(&g);
            let size = g.len();
            let this_cat = GraphemeCategory::from(&g);
            match self.graphemes.peek() {
                Some(next) => {
                    let next_cat = GraphemeCategory::from(next);
                    if (this_cat != GraphemeCategory::Whitespace && next_cat == GraphemeCategory::Whitespace) ||
                        (this_cat == GraphemeCategory::Whitespace && next_cat != GraphemeCategory::Whitespace) {
                        // that's the start of the current word
                        // and the index has to fall on the first
                        // column of a grapheme
                        let start_byte = offset.saturating_sub(size);

                        // start and end are reversed
                        let word = Range {
                            end_byte: self.offset,
                            start_byte,
                            end: self.col.saturating_sub(width - 1),
                            start: col.saturating_sub(width - 1),
                        };

                        self.col = col.saturating_sub(width);
                        self.offset = start_byte;

                        return Some(word)
                    }
                }
                None => {
                    // this is the start of the first word
                    // and the index has to fall on the first
                    // column of a grapheme
                    let start_byte = offset.saturating_sub(size);
                    return Some(Range {
                        end_byte: self.offset,
                        start_byte,
                        end: self.col,
                        start: col,
                    })
                }
            }

            col = col.saturating_sub(width);
            offset = offset.saturating_sub(size);
        }

        None
    }
}

#[cfg(test)]
mod test {
    use crop::Rope;
    use super::*;

    #[test]
    fn test_words() {
        let rope = Rope::from("Hello world, this is a test\nsecond line with (words) 😭😭😭😭 hi");
        let line = rope.line(1);
        let words = Words::new(line);
        // start, end, slice
        let expected = [
            (0, 5, "second"),
            (6, 6, " "),
            (7, 10, "line"),
            (11, 11, " "),
            (12, 15, "with"),
            (16, 16, " "),
            (17, 17, "("),
            (18, 22, "words"),
            (23, 23, ")"),
            (24, 24, " "),
            // remember: end falls on the first col of the last grapheme
            (25, 31, "😭😭😭😭"),
            (33, 33, " "),
            (34, 35, "hi"),
        ];
        for (word, expected) in words.zip(expected.into_iter()) {
            assert_eq!(word.start, expected.0, "\"{}\" starts on {} but shoud be {}", word.slice(line), word.start, expected.0);
            assert_eq!(word.end, expected.1, "\"{}\" ends on {} but shoud be {}", word.slice(line), word.end, expected.1);
            assert_eq!(word.slice(line), expected.2);
        }
    }

    #[test]
    fn test_words_backwards() {
        let rope = Rope::from("Hello world, this is a test\nsecond line with (words) 😭😭😭😭 hi");
        let line = rope.line(1);
        let words = WordsBackwards::new(line);
        let expected = [
            (34, 35, "hi"),
            (33, 33, " "),
            // remember: end falls on the first col of the last grapheme
            (25, 31, "😭😭😭😭"),
            (24, 24, " "),
            (23, 23, ")"),
            (18, 22, "words"),
            (17, 17, "("),
            (16, 16, " "),
            (12, 15, "with"),
            (11, 11, " "),
            (7, 10, "line"),
            (6, 6, " "),
            (0, 5, "second"),
        ];
        for (word, expected) in words.zip(expected.into_iter()) {
            assert_eq!(word.start, expected.0, "\"{}\" starts on {} but shoud be {}", word.slice(line), word.start, expected.0);
            assert_eq!(word.end, expected.1, "\"{}\" ends on {} but shoud be {}", word.slice(line), word.end, expected.1);
            assert_eq!(word.slice(line), expected.2);
        }
    }
}
