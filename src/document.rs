use std::{borrow::Cow, cmp::Ordering, path::PathBuf};

use crop::{Rope, RopeSlice};
use crate::editor::Mode;

enum HorizontalMove { Right, Left }
enum VerticalMove { Down, Up }
struct CursorMove {
    horizontal: Option<HorizontalMove>,
    vertical: Option<VerticalMove>,
}

pub const NEW_LINE: char = '\n';

fn move_direction(from: (usize, usize), to: (&usize, &usize)) -> CursorMove {
    CursorMove {
        horizontal: match from.0.cmp(to.0) {
            Ordering::Greater => Some(HorizontalMove::Left),
            Ordering::Less => Some(HorizontalMove::Right),
            Ordering::Equal => None,
        },
        vertical: match from.1.cmp(to.1) {
            Ordering::Greater => Some(VerticalMove::Up),
            Ordering::Less => Some(VerticalMove::Down),
            Ordering::Equal => None,
        }
    }
}

#[derive(Clone, Debug)]
struct Word<'a> {
    slice: RopeSlice<'a>,
    start: usize,
    end: usize,
}

impl<'a> Word<'a> {
    fn is_blank(&self) -> bool {
        self.slice.chars().all(|c| c.is_whitespace())
    }
}

pub struct Document {
    pub data: Rope,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub readonly: bool,
    sticky_cursor_x: usize,
}

impl Document {
    pub fn new(data: Rope, path: Option<PathBuf>) -> Self {
        let readonly = match &path {
            Some(p) => {
                std::fs::metadata(p).is_ok_and(|m| {
                    m.permissions().readonly()
                })
            },
            None => false,
        };
        Self {
            data,
            path,
            readonly,
            cursor_x: 0,
            cursor_y: 0,
            sticky_cursor_x: 0,
            modified: false,
        }
    }

    fn byte_offset_at_cursor(&self, cursor_x: usize, cursor_y: usize) -> usize {
        let mut offset = self.data.byte_of_line(cursor_y);
        let mut col = 0;
        for g in self.data.line(cursor_y).graphemes() {
            if col == cursor_x {
                break;
            }
            col += unicode_display_width::width(&g) as usize;
            offset += g.len();
        }
        offset
    }

    fn max_cursor_x(&self, line: usize, mode: &Mode) -> usize {
        match mode {
            Mode::Insert => self.line_len(line),
            Mode::Normal => self.line_len(line).saturating_sub(1),
        }
    }

    pub fn lines_len(&self) -> usize {
        self.data.lines().len()
    }

    pub fn line_len(&self, line: usize) -> usize {
        self.data.line(line).graphemes().map(|g| unicode_display_width::width(&g) as usize).sum()
    }

    pub fn current_line_len(&self) -> usize {
        self.line_len(self.cursor_y)
    }

    pub fn current_line(&self) -> RopeSlice {
        self.data.line(self.cursor_y)
    }

    pub fn insert_char_at_cursor(&mut self, char: char, mode: &Mode) {
        self.modified = true;
        let offset = self.byte_offset_at_cursor(self.cursor_x, self.cursor_y);
        let mut buf = [0; 4];
        let text = char.encode_utf8(&mut buf);

        self.data.insert(offset, text);

        if char == '\n' {
            self.move_cursor_to(Some(0), Some(self.cursor_y + 1), mode);
        } else {
            self.move_cursor_to(Some(self.cursor_x + 1), None, mode);
        }
    }

    pub fn grapheme_at_cursor(&self) -> (usize, Option<Cow<'_, str>>)  {
        let mut idx = 0;
        let mut col = 0;
        let mut grapheme = None;

        let mut iter = self.current_line().graphemes().enumerate().peekable();
        while let Some((i, g)) = iter.next() {
            idx = i;
            let width = unicode_display_width::width(&g) as usize;
            grapheme = Some(g);
            if col >= self.cursor_x { break }
            if iter.peek().is_none() { idx += 1 }
            col += width;
        }

        (idx, grapheme)
    }

    pub fn delete_to_the_left(&mut self, mode: &Mode) {
        if self.cursor_x > 0 {
            let mut start = self.data.byte_of_line(self.cursor_y);
            let mut end = start;
            let idx = self.grapheme_at_cursor().0 - 1;
            for (i, g) in self.current_line().graphemes().enumerate() {
                if i < idx { start += g.len() }
                if i == idx {
                    end = start + g.len();
                    break
                }
            }

            self.cursor_left(&Mode::Insert);
            self.data.delete(start..end);
            self.modified = true;
        } else if self.cursor_y > 0  {
            let to = self.data.byte_of_line(self.cursor_y);
            let from = to.saturating_sub(NEW_LINE.len_utf8());
            // need to move cursor before deleting
            self.move_cursor_to(Some(self.line_len(self.cursor_y - 1)), Some(self.cursor_y - 1), mode);
            self.data.delete(from..to);
            self.modified = true;
        }
    }

    pub fn delete_lines(&mut self, from: usize, to: usize, mode: &Mode) {
        let from_line = from.min(to);
        let to_line = from.max(to).min(self.lines_len().saturating_sub(1));

        let start = self.data.byte_of_line(from_line);
        let mut end = start + self.data.line(to_line).byte_len();

        if self.lines_len() > 1 {
            end += NEW_LINE.len_utf8();
        }

        if end > 0 {
            self.modified = true;
            self.data.delete(start..end);
            // if removing last line, go up
            if self.cursor_y > self.lines_len().saturating_sub(1) {
                self.cursor_up(mode);
            } else {
                // ensure x is within bounds
                self.move_cursor_to(None, None, mode);
            }
        }
    }

    pub fn delete_until_eol(&mut self, mode: &Mode) {
        let start = self.byte_offset_at_cursor(self.cursor_x, self.cursor_y);
        let end = self.data.byte_of_line(self.cursor_y) + self.current_line().byte_len();

        if end > 0 {
            self.data.delete(start..end);
            self.modified = true;
            self.move_cursor_to(None, None, mode);
        }
    }

    pub fn move_cursor_to(&mut self, x: Option<usize>, y: Option<usize>, mode: &Mode) {
        let stick = x.is_some();
        // ensure x and y are within bounds
        let y = self.lines_len().saturating_sub(1).min(y.unwrap_or(self.cursor_y));
        let x = self.max_cursor_x(y, mode).min(x.unwrap_or(self.sticky_cursor_x));

        let cursor_move = move_direction((self.cursor_x, self.cursor_y), (&x, &y));

        self.cursor_x = x;
        self.cursor_y = y;

        if x > 0 {
            self.ensure_cursor_is_on_grapheme_boundary(mode, cursor_move);
        }

        if stick { self.sticky_cursor_x = self.cursor_x }
    }

    fn ensure_cursor_is_on_grapheme_boundary(&mut self, mode: &Mode, cursor_move: CursorMove) {
        let mut acc = 0;
        let goto_prev = cursor_move.vertical.is_some() || matches!(cursor_move.horizontal, Some(HorizontalMove::Left));
        let goto_next = matches!(cursor_move.horizontal, Some(HorizontalMove::Right));

        let mut graphemes = self.current_line().graphemes().peekable();

        while let Some(g) = graphemes.next() {
            let width = unicode_display_width::width(&g) as usize;

            let next_grapheme_start = acc + width;

            if (self.cursor_x < next_grapheme_start) && (self.cursor_x > acc) {
                if goto_prev {
                    self.cursor_x = acc;
                } else if goto_next {
                    if graphemes.peek().is_none() && !matches!(mode, Mode::Insert) {
                        self.cursor_x = acc;
                    } else {
                        self.cursor_x = next_grapheme_start;
                    }
                }
                break;
            }

            acc += width;
        }
    }

    pub fn cursor_up(&mut self, mode: &Mode) {
        self.move_cursor_to(None, Some(self.cursor_y.saturating_sub(1)), mode);
    }

    pub fn cursor_down(&mut self, mode: &Mode) {
        self.move_cursor_to(None, Some(self.cursor_y + 1), mode);
    }

    pub fn cursor_left(&mut self, mode: &Mode) {
        self.move_cursor_to(Some(self.cursor_x.saturating_sub(1)), None, mode);
    }

    pub fn cursor_right(&mut self, mode: &Mode) {
        self.move_cursor_to(Some(self.cursor_x + 1), None, mode);
    }

    pub fn goto_line_first_non_whitespace(&mut self, line: usize, mode: &Mode) {
        for (i, g) in self.data.line(line).graphemes().enumerate() {
            if !matches!(GraphemeCategory::from(&g), GraphemeCategory::Whitespace) {
                self.move_cursor_to(Some(i), Some(line), mode);
                break;
            }
        }
    }

    fn words_of_line(&self, y: usize, exclude_blank_words: bool) -> Vec<Word> {
        let line = self.data.line(y);
        let mut offset = 0;
        let mut word_start_byte = offset;
        let mut words = vec![];
        let mut col = 0;
        let mut word = Word { start: col, end: col, slice: line.byte_slice(..) };
        let mut iter = line.graphemes().peekable();

        while let Some(g) = iter.next() {
            let width = unicode_display_width::width(&g) as usize;
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

    pub fn goto_word_end_forward(&mut self, mode: &Mode) {
        let mut line = self.cursor_y;

        'lines: while line < self.lines_len() {
            for word in self.words_of_line(line, true) {
                if line > self.cursor_y || self.cursor_x < word.end {
                    self.move_cursor_to(Some(word.end), Some(line), mode);
                    break 'lines;
                }
            }

            line += 1;
        }
    }

    pub fn goto_word_start_forward(&mut self, mode: &Mode) {
        let mut line = self.cursor_y;

        'lines: while line < self.lines_len() {
            for word in self.words_of_line(line, true) {
                if line > self.cursor_y || self.cursor_x < word.start {
                    self.move_cursor_to(Some(word.start), Some(line), mode);
                    break 'lines;
                }
            }

            line += 1;
        }
    }

    pub fn goto_word_start_backward(&mut self, mode: &Mode) {
        let mut line = self.cursor_y as isize;

        'lines: while line >= 0 {
            let l = line as usize;
            for word in self.words_of_line(l, true).iter().rev() {
                if l < self.cursor_y || self.cursor_x > word.start {
                    self.move_cursor_to(Some(word.start), Some(l), mode);
                    break 'lines;
                }
            }

            line -= 1;
        }
    }

    pub fn goto_word_end_backward(&mut self, mode: &Mode) {
        let mut line = self.cursor_y as isize;

        'lines: while line >= 0 {
            let l = line as usize;
            for word in self.words_of_line(l, true).iter().rev() {
                if l < self.cursor_y || self.cursor_x > word.end {
                    self.move_cursor_to(Some(word.end), Some(l), mode);
                    break 'lines;
                }
            }

            line -= 1;
        }
    }
}

#[derive(PartialEq)]
enum GraphemeCategory {
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
