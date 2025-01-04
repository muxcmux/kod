use crop::{Rope, RopeSlice};
use crossterm::event::KeyCode;

use crate::{graphemes::{width, GraphemeCategory}, selection::Selection};

pub fn words_of_line(rope: &Rope, y: usize, exclude_blank_words: bool) -> Vec<Word<'_>> {
    let line = rope.line(y);
    let mut offset = 0;
    let mut start_byte = offset;
    let mut words = vec![];
    let mut col = 0;
    let mut word = Word {
        start: col,
        end: col,
        start_byte,
        end_byte: start_byte,
        slice: line.byte_slice(..),
    };
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
                    word.slice = line.byte_slice(start_byte..offset + size);
                    word.start_byte = start_byte;
                    word.end_byte = offset + size;
                    // push it to the list of words
                    words.push(word.clone());
                    // start the next word
                    word.start = col + width;
                    start_byte = offset + size;
                }
            }
            None => {
                // this is the end of the last word
                // and the index has to fall on the first
                // column of a grapheme
                word.end = col;
                word.slice = line.byte_slice(start_byte..offset + size);
                word.start_byte = start_byte;
                word.end_byte = offset + size;
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
    pub start_byte: usize,
    pub end_byte: usize,
}

impl Word<'_> {
    pub fn is_blank(&self) -> bool {
        self.slice.chars().all(|c| c.is_whitespace())
    }

    fn contains(&self, col: &usize) -> bool {
        (self.start..=self.end).contains(col)
    }
}

pub enum Motion {
    Around,
    Inside,
}

#[derive(Debug)]
pub enum TextObject {
    Word,
    LongWord,
}

impl TryFrom<KeyCode> for TextObject {
    type Error = String;

    fn try_from(value: KeyCode) -> Result<Self, Self::Error> {
        match value {
            KeyCode::Char(c) => match c {
                'w' => Ok(Self::Word),
                'W' => Ok(Self::LongWord),
                _ => Err(format!("'{c}' does not map to a valid TextObject"))
            },
            _ => Err(format!("{value} does not map to a valid TextObject")),
        }
    }
}

impl TextObject {
    pub fn word<'a>(&self, rope: &'a Rope, selection: &Selection) -> Word<'a> {
        let words = words_of_line(rope, selection.head.y, false);
        words.into_iter().find(|w| w.contains(&selection.head.x)).unwrap()
    }
}
