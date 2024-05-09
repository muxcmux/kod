use std::{cmp::Ordering, path::PathBuf};

use crop::Rope;

use crate::editor::Mode;

enum HorizontalMove { Right, Left }
enum VerticalMove { Down, Up }
struct CursorMove {
    horizontal: Option<HorizontalMove>,
    vertical: Option<VerticalMove>,
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


pub struct Document {
    pub data: Rope,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub path: Option<PathBuf>,
    pub modified: bool,
    sticky_cursor_x: usize,
}

impl Document {
    pub fn new(data: Rope, path: Option<PathBuf>) -> Self {
        Self {
            data,
            path,
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

        let mut iter = self.data.line(self.cursor_y).graphemes().enumerate().peekable();
        while let Some((i, g)) = iter.next() {
            idx = i;
            if col >= self.cursor_x { break }
            if iter.peek().is_none() { idx += 1 }
            col += unicode_display_width::width(&g) as usize;
        }

        idx
    }

    pub fn delete_to_the_left(&mut self, mode: &Mode) {
        assert!(matches!(mode, Mode::Insert));

        self.modified = true;
        if self.cursor_x > 0 {
            let mut start = self.data.byte_of_line(self.cursor_y);
            let mut end = start;
            let idx = self.grapheme_idx_at_cursor() - 1;
            for (i, g) in self.data.line(self.cursor_y).graphemes().enumerate() {
                if i < idx { start += g.len() }
                if i == idx {
                    end = start + g.len();
                    break
                }
            }
            self.data.delete(start..end);
            self.cursor_left(&Mode::Insert);
        } else if self.cursor_y > 0 {
            let byte_length_of_newline_char = 1;
            let to = self.data.byte_of_line(self.cursor_y);
            let from = to.saturating_sub(byte_length_of_newline_char);
            // need to move cursor before deleting
            self.move_cursor_to(Some(self.line_len(self.cursor_y - 1)), Some(self.cursor_y - 1), mode);
            self.data.delete(from..to);
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

    pub fn move_cursor_to(&mut self, x: Option<usize>, y: Option<usize>, mode: &Mode) {
        // ensure x and y are within bounds
        let y = self.lines_len().saturating_sub(1).min(y.unwrap_or(self.cursor_y));
        let max_x = match mode {
            Mode::Insert => self.line_len(y),
            Mode::Normal => self.line_len(y).saturating_sub(1),
        };
        let x = max_x.min(x.unwrap_or(self.sticky_cursor_x));

        let cursor_move = move_direction((self.cursor_x, self.cursor_y), (&x, &y));

        self.cursor_x = x;
        self.cursor_y = y;

        self.ensure_cursor_is_on_grapheme_boundary(mode, cursor_move);
    }

    fn ensure_cursor_is_on_grapheme_boundary(&mut self, mode: &Mode, cursor_move: CursorMove) {
        let mut acc = 0;
        let go_to_prev = cursor_move.vertical.is_some() || matches!(cursor_move.horizontal, Some(HorizontalMove::Left));
        let go_to_next = matches!(cursor_move.horizontal, Some(HorizontalMove::Right));

        let mut graphemes = self.data.line(self.cursor_y).graphemes().peekable();

        while let Some(g) = graphemes.next() {
            let width = unicode_display_width::width(&g) as usize;

            let next_grapheme_start = acc + width;

            if (self.cursor_x < next_grapheme_start) && (self.cursor_x > acc) {
                if go_to_prev {
                    self.cursor_x = acc;
                } else if go_to_next {
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

