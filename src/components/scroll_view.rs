use std::{borrow::Cow, cmp::Ordering};

use crop::Rope;
use crossterm::style::Color;

use crate::{editor::Mode, graphemes::{GraphemeCategory, Word}, ui::{buffer::Buffer, Position, Rect}};

pub const NEW_LINE: char = '\n';

#[derive(PartialEq)]
enum HorizontalMove { Right, Left }
#[derive(PartialEq)]
enum VerticalMove { Down, Up }
struct CursorMove {
    horizontal: Option<HorizontalMove>,
    vertical: Option<VerticalMove>,
}

fn adjust_scroll(dimension: usize, doc_cursor: usize, offset: usize, scroll: usize) -> Option<usize> {
    if doc_cursor > dimension.saturating_sub(offset + 1) + scroll {
        return Some(doc_cursor.saturating_sub(dimension.saturating_sub(offset + 1)));
    }

    if doc_cursor < scroll + offset {
        return Some(doc_cursor.saturating_sub(offset));
    }

    None
}

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

#[derive(Default)]
pub struct ScrollView {
    pub view_cursor_position: Position,
    pub text_cursor_x: usize,
    pub text_cursor_y: usize,
    pub text_sticky_cursor_x: usize,
    pub offset_x: usize,
    pub offset_y: usize,
    pub scroll_x: usize,
    pub scroll_y: usize,
}

impl ScrollView {
    fn ensure_cursor_is_in_view(&mut self, area: Rect) {
        if let Some(s) = adjust_scroll(area.height as usize, self.text_cursor_y, self.offset_y, self.scroll_y) {
            self.scroll_y = s;
        }

        if let Some(s) = adjust_scroll(area.width as usize, self.text_cursor_x, self.offset_x, self.scroll_x) {
            self.scroll_x = s;
        }

        // adjust cursor
        self.view_cursor_position.y = area.top() + self.text_cursor_y.saturating_sub(self.scroll_y) as u16;
        self.view_cursor_position.x = area.left() + self.text_cursor_x.saturating_sub(self.scroll_x) as u16;
    }

    pub fn render<F>(&mut self, area: Rect, buffer: &mut Buffer, rope: &Rope, mut ws_callback: F)
        where F: FnMut(&mut Buffer, (u16, u16))
    {
        self.ensure_cursor_is_in_view(area);

        for row in self.scroll_y..self.scroll_y + area.height as usize {
            if row >= rope.line_len() {
                break;
            }
            let line = rope.line(row);
            let mut graphemes = line.graphemes();
            let mut skip_next_n_cols = 0;

            // advance the iterator to account for scroll
            let mut advance = 0;
            while advance < self.scroll_x {
                if let Some(g) = graphemes.next() {
                    advance += unicode_display_width::width(&g) as usize;
                    skip_next_n_cols = advance.saturating_sub(self.scroll_x);
                } else {
                    break
                }
            }

            let y = row.saturating_sub(self.scroll_y) as u16 + area.top();
            let mut trailing_whitespace = vec![];

            for col in self.scroll_x..self.scroll_x + area.width as usize {
                if skip_next_n_cols > 0 {
                    skip_next_n_cols -= 1;
                    continue;
                }
                match graphemes.next() {
                    None => break,
                    Some(g) => {
                        let width = unicode_display_width::width(&g) as usize;
                        let x = col.saturating_sub(self.scroll_x) as u16 + area.left();
                        buffer.put_symbol(&g, x, y, Color::Reset, Color::Reset);
                        skip_next_n_cols = width - 1;

                        if GraphemeCategory::from(&g) == GraphemeCategory::Whitespace {
                            trailing_whitespace.push(x);
                        } else {
                            trailing_whitespace.drain(..);
                        }
                    }
                }
            }

            for x in trailing_whitespace {
                ws_callback(buffer, (x, y));
            }
        }
    }

    pub fn byte_offset_at_cursor(&self, rope: &Rope, cursor_x: usize, cursor_y: usize) -> usize {
        let mut offset = rope.byte_of_line(cursor_y);
        let mut col = 0;
        for g in rope.line(cursor_y).graphemes() {
            if col == cursor_x {
                break;
            }
            col += unicode_display_width::width(&g) as usize;
            offset += g.len();
        }
        offset
    }

    fn max_cursor_x(&self, rope: &Rope, line: usize, mode: &Mode) -> usize {
        match mode {
            Mode::Insert | Mode::Replace => self.line_width(rope, line),
            Mode::Normal => self.line_width(rope, line).saturating_sub(1),
        }
    }

    pub fn is_blank(&self, rope: &Rope) -> bool {
        rope.is_empty() || *rope == NEW_LINE.to_string()
    }

    pub fn line_width(&self, rope: &Rope, line: usize) -> usize {
        rope.line(line).graphemes().map(|g| unicode_display_width::width(&g) as usize).sum()
    }

    // This needs to work with transactions
    pub fn insert_str_at_cursor(&mut self, rope: &mut Rope, str: &str, _mode: &Mode) {
        let offset = self.byte_offset_at_cursor(rope, self.text_cursor_x, self.text_cursor_y);
        rope.insert(offset, str);
        // TODO: Move the cursor
    }

    pub fn grapheme_at_cursor<'a>(&'a self, rope: &'a Rope) -> (usize, Option<Cow<'_, str>>)  {
        let mut idx = 0;
        let mut col = 0;
        let mut grapheme = None;

        let mut iter = rope.line(self.text_cursor_y).graphemes().enumerate().peekable();
        while let Some((i, g)) = iter.next() {
            idx = i;
            let width = unicode_display_width::width(&g) as usize;
            grapheme = Some(g);
            if col >= self.text_cursor_x { break }
            if iter.peek().is_none() { idx += 1 }
            col += width;
        }

        (idx, grapheme)
    }

    pub fn byte_range_until_eol(&mut self, rope: &Rope) -> Option<(usize, usize)> {
        let start = self.byte_offset_at_cursor(rope, self.text_cursor_x, self.text_cursor_y);
        let end = rope.byte_of_line(self.text_cursor_y) + rope.line(self.text_cursor_y).byte_len();

        if end > 0 {
            return Some((start, end));
        }

        None
    }

    /// Moves cursor to x and y,
    /// respecting bounds and grapheme boundaries
    pub fn move_cursor_to(&mut self, rope: &Rope, x: Option<usize>, y: Option<usize>, mode: &Mode) {
        let stick = x.is_some();
        // ensure x and y are within bounds
        let y = rope.line_len().saturating_sub(1).min(y.unwrap_or(self.text_cursor_y));
        let x = self.max_cursor_x(rope, y, mode).min(x.unwrap_or(self.text_sticky_cursor_x));

        let cursor_move = move_direction((self.text_cursor_x, self.text_cursor_y), (&x, &y));

        self.text_cursor_x = x;
        self.text_cursor_y = y;

        if x > 0 {
            self.ensure_cursor_is_on_grapheme_boundary(rope, mode, cursor_move);
        }

        if stick { self.text_sticky_cursor_x = self.text_cursor_x }
    }

    fn ensure_cursor_is_on_grapheme_boundary(&mut self, rope: &Rope, mode: &Mode, cursor_move: CursorMove) {
        let mut acc = 0;
        let mut goto_prev = cursor_move.vertical.is_some() || cursor_move.horizontal == Some(HorizontalMove::Left);
        let goto_next = cursor_move.horizontal == Some(HorizontalMove::Right);

        if !goto_next && !goto_prev { goto_prev = true }

        let mut graphemes = rope.line(self.text_cursor_y).graphemes().peekable();

        while let Some(g) = graphemes.next() {
            let width = unicode_display_width::width(&g) as usize;

            let next_grapheme_start = acc + width;

            if (self.text_cursor_x < next_grapheme_start) && (self.text_cursor_x > acc) {
                if goto_prev {
                    self.text_cursor_x = acc;
                } else if goto_next {
                    if graphemes.peek().is_none() && mode != &Mode::Insert {
                        self.text_cursor_x = acc;
                    } else {
                        self.text_cursor_x = next_grapheme_start;
                    }
                }
                break;
            }

            acc += width;
        }
    }

    pub fn cursor_up(&mut self, rope: &Rope, mode: &Mode) {
        self.move_cursor_to(rope, None, Some(self.text_cursor_y.saturating_sub(1)), mode);
    }

    pub fn cursor_down(&mut self, rope: &Rope, mode: &Mode) {
        self.move_cursor_to(rope, None, Some(self.text_cursor_y + 1), mode);
    }

    pub fn cursor_left(&mut self, rope: &Rope, mode: &Mode) {
        self.move_cursor_to(rope, Some(self.text_cursor_x.saturating_sub(1)), None, mode);
    }

    pub fn cursor_right(&mut self, rope: &Rope, mode: &Mode) {
        self.move_cursor_to(rope, Some(self.text_cursor_x + 1), None, mode);
    }

    pub fn goto_line_first_non_whitespace(&mut self, rope: &Rope, line: usize, mode: &Mode) {
        for (i, g) in rope.line(line).graphemes().enumerate() {
            if GraphemeCategory::from(&g) != GraphemeCategory::Whitespace {
                self.move_cursor_to(rope, Some(i), Some(line), mode);
                break;
            }
        }
    }

    fn words_of_line<'a>(&'a self, rope: &'a Rope, y: usize, exclude_blank_words: bool) -> Vec<Word> {
        let line = rope.line(y);
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

    pub fn goto_word_end_forward(&mut self, rope: &Rope, mode: &Mode) {
        let mut line = self.text_cursor_y;

        'lines: while line < rope.line_len() {
            for word in self.words_of_line(rope, line, true) {
                if line > self.text_cursor_y || self.text_cursor_x < word.end {
                    self.move_cursor_to(rope, Some(word.end), Some(line), mode);
                    break 'lines;
                }
            }

            line += 1;
        }
    }

    pub fn goto_word_start_forward(&mut self, rope: &Rope, mode: &Mode) {
        let mut line = self.text_cursor_y;

        'lines: while line < rope.line_len() {
            for word in self.words_of_line(rope, line, true) {
                if line > self.text_cursor_y || self.text_cursor_x < word.start {
                    self.move_cursor_to(rope, Some(word.start), Some(line), mode);
                    break 'lines;
                }
            }

            line += 1;
        }
    }

    pub fn goto_word_start_backward(&mut self, rope: &Rope, mode: &Mode) {
        let mut line = self.text_cursor_y as isize;

        'lines: while line >= 0 {
            let l = line as usize;
            for word in self.words_of_line(rope, l, true).iter().rev() {
                if l < self.text_cursor_y || self.text_cursor_x > word.start {
                    self.move_cursor_to(rope, Some(word.start), Some(l), mode);
                    break 'lines;
                }
            }

            line -= 1;
        }
    }

    pub fn goto_word_end_backward(&mut self, rope: &Rope, mode: &Mode) {
        let mut line = self.text_cursor_y as isize;

        'lines: while line >= 0 {
            let l = line as usize;
            for word in self.words_of_line(rope, l, true).iter().rev() {
                if l < self.text_cursor_y || self.text_cursor_x > word.end {
                    self.move_cursor_to(rope, Some(word.end), Some(l), mode);
                    break 'lines;
                }
            }

            line -= 1;
        }
    }

    pub fn goto_character_forward(&mut self, rope: &Rope, c: char, mode: &Mode, offset: usize) {
        let mut col = 0;
        for g in rope.line(self.text_cursor_y).graphemes() {
            if col > self.text_cursor_x && g.starts_with(c) {
                self.move_cursor_to(rope, Some(col.saturating_sub(offset)), None, mode);
                break;
            }
            let width = unicode_display_width::width(&g) as usize;
            col += width;
        }
    }

    pub fn goto_character_backward(&mut self, rope: &Rope, c: char, mode: &Mode, offset: usize) {
        let mut col = self.line_width(rope, self.text_cursor_y);
        for g in rope.line(self.text_cursor_y).graphemes().rev() {
            if col <= self.text_cursor_x && g.starts_with(c) {
                self.move_cursor_to(rope, Some(col.saturating_sub(offset)), None, mode);
                break;
            }
            let width = unicode_display_width::width(&g) as usize;
            col -= width;
        }
    }
}
