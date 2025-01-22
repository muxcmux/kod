use std::{borrow::Cow, ops::Range};

use crop::Rope;

use crate::{editor::Mode, graphemes::{self, line_width}};

// Represents a virtual cursor position in a text rope with
// absolute positions 0, 0 from the first line/ first col
// in a text rope. This always needs to be grapheme aligned
#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering::*;
        if self.y < other.y { return Some(Less) }
        if self.y > other.y { return Some(Greater) }
        if self.x < other.x { return Some(Less) }
        if self.x > other.x { return Some(Greater) }
        Some(Equal)
    }
}

impl Cursor {
    fn min<'a>(&'a self, other: &'a Self) -> &'a Self {
        match self.partial_cmp(other).unwrap() {
            std::cmp::Ordering::Greater => other,
            _ => self,
        }
    }

    fn max<'a>(&'a self, other: &'a Self) -> &'a Self {
        match self.partial_cmp(other).unwrap() {
            std::cmp::Ordering::Less => other,
            _ => self,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct Selection {
    // the point which doesn't move
    pub anchor: Cursor,
    // the point that moves when extending/shrinking a selection
    pub head: Cursor,
    // used to save the max x position for vertical movement
    pub sticky_x: usize,
}

// Pulibc methods on this that return new selections
// need to be grapheme aligned
impl Selection {
    fn range(&self) -> (&Cursor, &Cursor) {
        (
            self.head.min(&self.anchor),
            self.head.max(&self.anchor)
        )
    }

    pub fn contains_cursor(&self, x: usize, y: usize) -> bool {
        let cursor = Cursor {x, y};
        let (min, max) = self.range();
        cursor >= *min && cursor <= *max
    }

    pub fn invert(&self) -> Self {
        Self {
            head: self.anchor,
            anchor: self.head,
            sticky_x: self.anchor.x,
        }
    }

    /// Moves the head to x and y,
    /// respecting bounds and grapheme boundaries
    pub fn head_to(self, rope: &Rope, x: Option<usize>, y: Option<usize>, mode: &Mode) -> Self {
        let stick = x.is_some();
        // ensure x and y are within bounds
        let y = rope.line_len().saturating_sub(1).min(y.unwrap_or(self.head.y));
        let x = max_cursor_x(rope, y, mode).min(x.unwrap_or(self.sticky_x));

        let cursor_move = move_direction((self.head.x, self.head.y), (&x, &y));

        let sticky_x = if stick { x } else { self.sticky_x };

        Self {
            head: Cursor { x, y },
            sticky_x,
            ..self
        }
        .grapheme_aligned(rope, mode, cursor_move)
    }

    pub fn up(self, rope: &Rope, mode: &Mode) -> Self {
        self.head_to(rope, None, Some(self.head.y.saturating_sub(1)), mode)
    }

    pub fn down(self, rope: &Rope, mode: &Mode) -> Self {
        self.head_to(rope, None, Some(self.head.y + 1), mode)
    }

    pub fn left(self, rope: &Rope, mode: &Mode) -> Self {
        self.head_to(rope, Some(self.head.x.saturating_sub(1)), None, mode)
    }

    pub fn right(self, rope: &Rope, mode: &Mode) -> Self {
        self.head_to(rope, Some(self.head.x + 1), None, mode)
    }

    pub fn anchor(self) -> Self {
        Self {
            anchor: self.head,
            ..self
        }
    }

    pub fn collapse_to_smaller_end(self) -> Self {
        let head = *self.head.min(&self.anchor);
        Self {
            head,
            anchor: head,
            sticky_x: head.x,
        }
    }

    pub fn collapse_to_head(self) -> Self {
        Self {
            anchor: self.head,
            ..self
        }
    }

    // Currently only accounts for the head
    fn grapheme_aligned(self, rope: &Rope, mode: &Mode, cursor_move: Direction) -> Self {
        let mut col = 0;
        let mut goto_prev = cursor_move.vertical.is_some() || cursor_move.horizontal == Some(Horizontal::Left);
        let goto_next = cursor_move.horizontal == Some(Horizontal::Right);

        if !goto_next && !goto_prev { goto_prev = true }

        let mut sel = self;

        let mut graphemes = rope.line(sel.head.y).graphemes().peekable();

        while let Some(g) = graphemes.next() {
            let width = graphemes::width(&g);

            let next_grapheme_start = col + width;

            if sel.head.x + width == next_grapheme_start {
                return sel;
            }

            if (sel.head.x < next_grapheme_start) && (sel.head.x > col) {
                if goto_prev {
                    sel.head.x = col;
                } else if goto_next {
                    if graphemes.peek().is_none() && mode != &Mode::Insert {
                        sel.head.x = col;
                    } else {
                        sel.head.x = next_grapheme_start;
                    }
                }
                break;
            }

            col += width;
        }

        sel
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

    pub fn head_at_byte(&self, rope: &Rope, byte: usize) -> Cursor {
        let (mut x, y) = (0, rope.line_of_byte(byte));
        let line = rope.line(y);
        let mut offset = rope.byte_of_line(y);
        for g in line.graphemes() {
            if offset >= byte { break }

            x += graphemes::width(&g);

            offset += g.bytes().len();
        }

        Cursor { x, y }
    }

    pub fn byte_range(&self, rope: &Rope, inclusive: bool, include_eol: bool) -> Range<usize> {
        let (start, end) = self.range();
        let (start, end) = (
            byte_offset_at_cursor(rope, start, false, false),
            byte_offset_at_cursor(rope, end, inclusive, include_eol),
        );
        start..end
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
        Mode::Normal => line_width(rope, line).saturating_sub(1),
        _ => line_width(rope, line),
    }
}

/// Returns the byte offset from the cursor
/// inclusive: true includes the lenght of the grapheme the cursor is currently at
/// include_eol: true includes the length of the new line character
/// NOTE: This does not go past the LAST grapheme on the LAST line
fn byte_offset_at_cursor(
    rope: &Rope,
    cursor: &Cursor,
    inclusive: bool,
    include_eol: bool,
) -> usize {
    let mut offset = rope.byte_of_line(cursor.y);
    let mut col = 0;
    let mut cursor_is_past_last_grapheme = true;

    for g in rope.line(cursor.y).graphemes() {
        if col == cursor.x {
            if inclusive {
                offset += g.len();
            }
            cursor_is_past_last_grapheme = false;
            break;
        }
        col += graphemes::width(&g);
        offset += g.len();
    }

    // In select mode the cursor can go after the last grapheme
    // just like insert mode. This indicates that the selection
    // includes the line ending, so we need to include that byte
    // in the range too
    let is_last_line = cursor.y == rope.line_len() - 1;
    if inclusive && cursor_is_past_last_grapheme && include_eol && !is_last_line {
        offset = rope.line_slice(..cursor.y + 1).byte_len();
    }

    offset
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_compare_cursors() {
        assert!(Cursor { x: 0, y: 0 } < Cursor { x: 5, y: 0 });
        assert!(Cursor { x: 0, y: 1 } > Cursor { x: 5, y: 0 });
        assert!(Cursor { x: 5, y: 0 } == Cursor { x: 5, y: 0 });
    }

    #[test]
    fn min_max() {

        let one = Cursor { x: 5, y: 0 };
        let two = Cursor { x: 1, y: 0 };
        let three = Cursor { x: 0, y: 1 };

        assert_eq!(one.min(&one), &one);
        assert_eq!(one.min(&two), &two);
        assert_eq!(one.min(&three), &one);
        assert_eq!(one.max(&one), &one);
        assert_eq!(one.max(&two), &one);
        assert_eq!(one.max(&three), &three);
    }
}
