use std::iter::{Peekable, Rev};

use crop::{iter::Graphemes, Rope, RopeSlice};
use crossterm::event::KeyCode;

use crate::{graphemes::{width, GraphemeCategory}, selection};

// Need to expand this to account for starting and ending row as well
#[derive(Debug)]
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
    Quotes(char),
    Pairs(char),
}

impl TryFrom<KeyCode> for TextObjectKind {
    type Error = String;

    fn try_from(value: KeyCode) -> Result<Self, Self::Error> {
        match value {
            KeyCode::Char(c) => match c {
                'w' => Ok(Self::Word),
                'W' => Ok(Self::LongWord),
                '"' | '\'' | '`' => Ok(Self::Quotes(c)),
                '{' | '}' | '[' | ']' | '(' | ')' | '<' | '>' => Ok(Self::Pairs(c)),
                _ => Err(format!("'{c}' does not map to a valid TextObjectKind"))
            },
            _ => Err(format!("{value} does not map to a valid TextObjectKind")),
        }
    }
}

impl TextObjectKind {
    pub fn inside(&self, rope: &Rope, range: &selection::Range) -> Option<Range> {
        match self {
            Self::Word => {
                let mut words = Words::new(rope.line(range.head.y));
                words.find(|w| w.contains(&range.head.x))
            },
            Self::LongWord => {
                let mut words = LongWords::new(rope.line(range.head.y));
                words.find(|w| w.contains(&range.head.x))
            },
            Self::Quotes(c) => {
                let mut quotes = Quotes::new(c, rope.line(range.head.y));
                quotes.find(|q| q.contains(&range.head.x) || q.start >= range.head.x)
            },
            Self::Pairs(_c) => todo!()
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

struct Quotes<'a> {
    quote: String,
    offset: usize,
    col: usize,
    graphemes: Graphemes<'a>,
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

impl<'a> Quotes<'a> {
    pub fn new(quote: &char, slice: RopeSlice<'a>) -> Self {
        Self {
            quote: quote.to_string(),
            col: 0,
            offset: 0,
            graphemes: slice.graphemes(),
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

impl Iterator for Quotes<'_> {

    type Item = Range;
    fn next(&mut self) -> Option<Self::Item> {
        let mut found_start = false;
        let mut col = self.col;
        let mut offset = self.offset;
        let mut range = Range { start: col, start_byte: offset, end: col, end_byte: offset };

        for g in self.graphemes.by_ref() {
            let width = width(&g);
            let size = g.len();
            col += width;
            offset += size;

            if g == self.quote {
                if found_start {
                    range.end = col.saturating_sub(width);
                    range.end_byte = offset;
                    self.col = col;
                    self.offset = offset;
                    return Some(range);
                }

                range.start = col.saturating_sub(width);
                range.start_byte = offset.saturating_sub(size);
                found_start = true;
            }

            self.col = col;
            self.offset = offset;
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

    #[test]
    fn test_quotes() {
        let rope = Rope::from("Hello world, this is a test\nsecond 'line' 'with' (words) '😭😭😭😭' hi it's me, Mario");
        let line = rope.line(1);
        let quotes = Quotes::new(&'\'', line);
        let expected = [
            (7, 12, "'line'"),
            (14, 19, "'with'"),
            (29, 38, "'😭😭😭😭'"),
        ];
        for (quote, expected) in quotes.zip(expected.into_iter()) {
            assert_eq!(quote.start, expected.0, "\"{}\" starts on {} but shoud be {}", quote.slice(line), quote.start, expected.0);
            assert_eq!(quote.end, expected.1, "\"{}\" ends on {} but shoud be {}", quote.slice(line), quote.end, expected.1);
            assert_eq!(quote.slice(line), expected.2);
        }
    }
}
