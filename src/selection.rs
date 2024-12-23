use std::{borrow::Cow, usize};

use crop::Rope;

use crate::{editor::Mode, graphemes::{self, line_width, words_of_line, GraphemeCategory}};

// Represents a virtual cursor position in a text rope with
// absolute positions 0, 0 from the first line/ first col
// in a text rope. This always needs to be grapheme aligned
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
}

// #[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
// enum SelectionKind {
//     #[default]
//     None,
//     Grapheme,
//     Line,
// }

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct Selection {
    // kind: SelectionKind,
    // the point which doesn't move
    // pub anchor: TextCursor,
    // the point that moves when extending/shrinking a selection
    pub head: Cursor,
    // used to save the max x position for vertical movement
    pub sticky_x: usize,
}

// Pulibc methods on this that return new selections
// need to be grapheme aligned
impl Selection {
    /// Moves the head to x and y,
    /// respecting bounds and grapheme boundaries
    pub fn move_to(&self, rope: &Rope, x: Option<usize>, y: Option<usize>, mode: &Mode) -> Selection {
        let stick = x.is_some();
        // ensure x and y are within bounds
        let y = rope.line_len().saturating_sub(1).min(y.unwrap_or(self.head.y));
        let x = max_cursor_x(rope, y, mode).min(x.unwrap_or(self.sticky_x));

        let cursor_move = move_direction((self.head.x, self.head.y), (&x, &y));

        let sticky_x = if stick { x } else { self.sticky_x };

        Self {
            head: Cursor { x, y },
            sticky_x,
            ..*self
        }
        .grapheme_aligned(rope, mode, cursor_move)
    }

    pub fn up(&self, rope: &Rope, mode: &Mode) -> Self {
        self.move_to(rope, None, Some(self.head.y.saturating_sub(1)), mode)
    }

    pub fn down(&self, rope: &Rope, mode: &Mode) -> Self {
        self.move_to(rope, None, Some(self.head.y + 1), mode)
    }

    pub fn left(&self, rope: &Rope, mode: &Mode) -> Self {
        self.move_to(rope, Some(self.head.x.saturating_sub(1)), None, mode)
    }

    pub fn right(&self, rope: &Rope, mode: &Mode) -> Self {
        self.move_to(rope, Some(self.head.x + 1), None, mode)
    }

    pub fn goto_word_end_forward(&self, rope: &Rope, mode: &Mode) -> Selection {
        let mut line = self.head.y;

        while line < rope.line_len() {
            for word in words_of_line(rope, line, true) {
                if line > self.head.y || self.head.x < word.end {
                    return self.move_to(rope, Some(word.end), Some(line), mode);
                }
            }

            line += 1;
        }

        self.move_to(rope, Some(usize::MAX), Some(rope.line_len().saturating_sub(1)), mode)
    }

    pub fn goto_word_start_forward(&self, rope: &Rope, mode: &Mode) -> Selection {
        let mut line = self.head.y;

        while line < rope.line_len() {
            for word in words_of_line(rope, line, true) {
                if line > self.head.y || self.head.x < word.start {
                    return self.move_to(rope, Some(word.start), Some(line), mode);
                }
            }

            line += 1;
        }

        self.move_to(rope, Some(usize::MAX), Some(rope.line_len().saturating_sub(1)), mode)
    }

    pub fn goto_word_start_backward(&self, rope: &Rope, mode: &Mode) -> Selection {
        let mut line = self.head.y as isize;

        while line >= 0 {
            let l = line as usize;
            for word in words_of_line(rope, l, true).iter().rev() {
                if l < self.head.y || self.head.x > word.start {
                    return self.move_to(rope, Some(word.start), Some(l), mode);
                }
            }

            line -= 1;
        }

        self.move_to(rope, Some(0), Some(0), mode)
    }

    pub fn goto_word_end_backward(&self, rope: &Rope, mode: &Mode) -> Selection {
        let mut line = self.head.y as isize;

        while line >= 0 {
            let l = line as usize;
            for word in words_of_line(rope, l, true).iter().rev() {
                if l < self.head.y || self.head.x > word.end {
                    return self.move_to(rope, Some(word.end), Some(l), mode);
                }
            }

            line -= 1;
        }

        self.move_to(rope, Some(0), Some(0), mode)
    }

    pub fn goto_line_first_non_whitespace(&self, rope: &Rope, line: Option<usize>, mode: &Mode) -> Selection {
        let line = line.unwrap_or(self.head.y);
        for (i, g) in rope.line(line).graphemes().enumerate() {
            if GraphemeCategory::from(&g) != GraphemeCategory::Whitespace {
                return self.move_to(rope, Some(i), Some(line), mode);
            }
        }

        unreachable!();
    }

    // Currently only accounts for the head
    fn grapheme_aligned(&self, rope: &Rope, mode: &Mode, cursor_move: Direction) -> Self {
        let mut acc = 0;
        let mut goto_prev = cursor_move.vertical.is_some() || cursor_move.horizontal == Some(Horizontal::Left);
        let goto_next = cursor_move.horizontal == Some(Horizontal::Right);

        if !goto_next && !goto_prev { goto_prev = true }

        let mut selection = self.clone();

        let mut graphemes = rope.line(selection.head.y).graphemes().peekable();

        while let Some(g) = graphemes.next() {
            let width = graphemes::width(&g);

            let next_grapheme_start = acc + width;

            if (selection.head.x < next_grapheme_start) && (selection.head.x > acc) {
                if goto_prev {
                    selection.head.x = acc;
                } else if goto_next {
                    if graphemes.peek().is_none() && mode != &Mode::Insert {
                        selection.head.x = acc;
                    } else {
                        selection.head.x = next_grapheme_start;
                    }
                }
                break;
            }

            acc += width;
        }

        selection
    }

    pub fn byte_offset_at_head(&self, rope: &Rope) -> usize {
        let mut offset = rope.byte_of_line(self.head.y);
        let mut col = 0;
        for g in rope.line(self.head.y).graphemes() {
            if col == self.head.x {
                break;
            }
            col += graphemes::width(&g);
            offset += g.len();
        }
        offset
    }

    pub fn grapheme_at_head<'a>(&'a self, rope: &'a Rope) -> (usize, Option<Cow<'a, str>>)  {
        let mut idx = 0;
        let mut col = 0;
        let mut grapheme = None;

        let mut iter = rope.line(self.head.y).graphemes().enumerate().peekable();
        while let Some((i, g)) = iter.next() {
            idx = i;
            let width = graphemes::width(&g);
            grapheme = Some(g);
            if col >= self.head.x { break }
            if iter.peek().is_none() { idx += 1 }
            col += width;
        }

        (idx, grapheme)
    }

    pub fn head_at_byte(&self, rope: &Rope, byte: usize) -> (usize, usize) {
        let (mut cursor_x, cursor_y) = (0, rope.line_of_byte(byte));
        let line = rope.line(cursor_y);
        let mut offset = rope.byte_of_line(cursor_y);
        for g in line.graphemes() {
            if offset >= byte { break }

            cursor_x += graphemes::width(&g);

            offset += g.bytes().len();
        }

        (cursor_x, cursor_y)
    }
}

#[derive(PartialEq)]
enum Horizontal { Right, Left }
#[derive(PartialEq)]
enum Vertical { Down, Up }
struct Direction {
    horizontal: Option<Horizontal>,
    vertical: Option<Vertical>,
}

fn move_direction(from: (usize, usize), to: (&usize, &usize)) -> Direction {
    use std::cmp::Ordering::{Greater, Less, Equal};

    Direction {
        horizontal: match from.0.cmp(to.0) {
            Greater => Some(Horizontal::Left),
            Less => Some(Horizontal::Right),
            Equal => None,
        },
        vertical: match from.1.cmp(to.1) {
            Greater => Some(Vertical::Up),
            Less => Some(Vertical::Down),
            Equal => None,
        }
    }
}

fn max_cursor_x(rope: &Rope, line: usize, mode: &Mode) -> usize {
    match mode {
        Mode::Insert | Mode::Replace => line_width(rope, line),
        Mode::Normal => line_width(rope, line).saturating_sub(1),
    }
}

