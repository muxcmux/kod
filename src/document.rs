use std::{cmp::Ordering, path::PathBuf};

use crop::{Rope, RopeSlice};
use log::debug;

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

    pub fn grapheme_idx_at_cursor(&self) -> usize {
        let mut idx = 0;
        let mut col = 0;

        let mut iter = self.current_line().graphemes().enumerate().peekable();
        while let Some((i, g)) = iter.next() {
            idx = i;
            if col >= self.cursor_x { break }
            if iter.peek().is_none() { idx += 1 }
            col += unicode_display_width::width(&g) as usize;
        }

        idx
    }

    pub fn delete_to_the_left(&mut self, mode: &Mode) {
        if self.cursor_x > 0 {
            let mut start = self.data.byte_of_line(self.cursor_y);
            let mut end = start;
            let idx = self.grapheme_idx_at_cursor() - 1;
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

    pub fn move_cursor_to(&mut self, x: Option<usize>, y: Option<usize>, mode: &Mode) {
        // ensure x and y are within bounds
        let y = self.lines_len().saturating_sub(1).min(y.unwrap_or(self.cursor_y));
        let x = self.max_cursor_x(y, mode).min(x.unwrap_or(self.sticky_cursor_x));

        let cursor_move = move_direction((self.cursor_x, self.cursor_y), (&x, &y));

        self.cursor_x = x;
        self.cursor_y = y;

        self.ensure_cursor_is_on_grapheme_boundary(mode, cursor_move);
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

        self.sticky_cursor_x = self.cursor_x;
    }

    pub fn cursor_right(&mut self, mode: &Mode) {
        self.move_cursor_to(Some(self.cursor_x + 1), None, mode);

        self.sticky_cursor_x = self.cursor_x;
    }
}

