use crop::Rope;
use smallvec::SmallVec;

use crate::{editor::Mode, graphemes::{self, grapheme_is_line_ending, line_width}};

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
        Some(self.cmp(other))
    }
}

impl Ord for Cursor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering::*;
        if self.y < other.y { return Less }
        if self.y > other.y { return Greater }
        if self.x < other.x { return Less }
        if self.x > other.x { return Greater }
        Equal
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default)]
pub struct Range {
    // the point which doesn't move
    pub anchor: Cursor,
    // the point that moves when extending/shrinking a selection
    pub head: Cursor,
    // used to save the max x position for vertical movement
    pub sticky_x: usize,
}

impl Range {
    // Creates a new range from a byte range in a doc setting the
    // head at the start of the range and the anchor and the end
    pub fn from_byte_range(rope: &Rope, byte_range: std::ops::Range<usize>) -> Self {
        debug_assert!(byte_range.end <= rope.byte_len());

        let mut range = Self::default();
        range.head.y = rope.line_of_byte(byte_range.start);
        let mut offset = rope.byte_of_line(range.head.y);

        // get to the first grapheme and set the head to it
        for g in rope.line(range.head.y).graphemes() {
            if offset >= byte_range.start { break }
            offset += g.len();
            range.head.x += graphemes::width(&g);
        }
        range.sticky_x = range.head.x;
        range.anchor = range.head;

        let end = byte_range.end;
        for g in rope.byte_slice(byte_range).graphemes() {
            offset += g.len();
            if offset >= end { break }
            if grapheme_is_line_ending(&g) {
                range.anchor.x = 0;
                range.anchor.y += 1;
            } else {
                range.anchor.x += graphemes::width(&g);
            }
        }

        range
    }

    pub fn from(&self) -> Cursor {
        self.head.min(self.anchor)
    }

    pub fn to(&self) -> Cursor {
        self.head.max(self.anchor)
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.from() == other.from() || (self.to() >= other.from() && other.to() >= self.from())
    }

    /// Returns a range that encompasses both input ranges.
    fn merge(self, other: Self) -> Self {
        if self.anchor > self.head && other.anchor > other.head {
            Range {
                anchor: self.anchor.max(other.anchor),
                head: self.head.min(other.head),
                sticky_x: self.sticky_x.max(other.sticky_x)
            }
        } else {
            Range {
                anchor: self.from().min(other.from()),
                head: self.to().max(other.to()),
                sticky_x: self.sticky_x.max(other.sticky_x)
            }
        }
    }

    pub fn contains_cursor(&self, x: usize, y: usize) -> bool {
        let cursor = Cursor {x, y};
        cursor >= self.from() && cursor <= self.to()
    }

    pub fn flip(self) -> Self {
        Self {
            head: self.anchor,
            anchor: self.head,
            sticky_x: self.anchor.x,
        }
    }

    /// Moves to x and y respecting bounds and grapheme boundaries
    /// Select mode only moves the head, while other modes move both ends
    pub fn move_to(self, rope: &Rope, x: Option<usize>, y: Option<usize>, mode: &Mode) -> Self {
        let stick = x.is_some();
        // ensure x and y are within bounds
        let y = rope.line_len().saturating_sub(1).min(y.unwrap_or(self.head.y));
        let x = max_cursor_x(rope, y, mode).min(x.unwrap_or(self.sticky_x));

        let cursor_move = move_direction((self.head.x, self.head.y), (&x, &y));

        let sticky_x = if stick { x } else { self.sticky_x };

        let aligned = Self {
            head: Cursor { x, y },
            sticky_x,
            ..self
        }.grapheme_aligned(rope, mode, cursor_move);

        // Non-select modes move both the head and the anchor at the same time
        if mode != &Mode::Select {
            return aligned.collapse_to_head()
        }

        aligned
    }

    pub fn up(self, rope: &Rope, mode: &Mode) -> Self {
        let y = self.head.y.saturating_sub(1);
        self.move_to(rope, None, Some(y), mode)
    }

    pub fn down(self, rope: &Rope, mode: &Mode) -> Self {
        let y = self.head.y + 1;
        self.move_to(rope, None, Some(y), mode)
    }

    pub fn left(self, rope: &Rope, mode: &Mode) -> Self {
        let x = self.head.x.saturating_sub(1);
        self.move_to(rope, Some(x), None, mode)
    }

    pub fn right(self, rope: &Rope, mode: &Mode) -> Self {
        let x = self.head.x + 1;
        self.move_to(rope, Some(x), None, mode)
    }

    pub fn anchor(self) -> Self {
        Self {
            anchor: self.head,
            ..self
        }
    }

    pub fn collapse_to_head(self) -> Self {
        Self {
            anchor: self.head,
            ..self
        }
    }

    pub fn collapse_to_start(self) -> Self {
        let start = self.from();
        Self {
            anchor: start,
            head: start,
            sticky_x: start.x,
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

    // Returns the byte range covered by a cursor.
    //
    // When the start of the selection is on the first column of a line
    // and the end is beyond the last grapheme on the last line, we extend
    // the start to cover the new line byte of the previous line (if any)
    // which in Select mode is usually the desired behaviour.
    pub fn byte_range(&self, rope: &Rope, mode: &Mode) -> std::ops::Range<usize> {
        let from = self.from();
        let to = self.to();

        let start = if mode == &Mode::Select &&
            from.x == 0 &&
            from.y > 0 &&
            to.y == rope.line_len().saturating_sub(1) &&
            to.x == max_cursor_x(rope, to.y, &Mode::Select) {
            rope.byte_of_line(from.y - 1) + rope.line(from.y - 1).byte_len()
        } else {
            byte_offset_at_cursor(rope, &from, &Mode::Normal)
        };

        let end = byte_offset_at_cursor(rope, &to, mode);

        start..end
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Selection {
    pub ranges: SmallVec<[Range; 1]>,
    pub primary_index: usize,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            primary_index: 0,
            ranges: SmallVec::from([Range::default()])
        }
    }
}

impl Selection {
    /// Ensure selection containing only the primary selection.
    pub fn into_single(self) -> Self {
        if self.ranges.len() == 1 {
            self
        } else {
            Self {
                ranges: SmallVec::from([self.ranges[self.primary_index]]),
                primary_index: 0,
            }
        }
    }

    pub fn primary(&self) -> &Range {
        &self.ranges[self.primary_index]
    }

    /// Removes a range from the selection.
    pub fn remove(mut self, index: usize) -> Self {
        debug_assert!(
            self.ranges.len() > 1,
            "can't remove the last range from a selection!"
        );

        self.ranges.remove(index);
        if index < self.primary_index || self.primary_index == self.ranges.len() {
            self.primary_index -= 1;
        }
        self
    }

    /// Takes a closure and maps each `Range` over the closure.
    pub fn transform<F>(&self, mut f: F) -> Self
    where
        F: FnMut(Range) -> Range,
    {
        let mut new = self.clone();
        for range in new.ranges.iter_mut() {
            *range = f(*range)
        }
        new.normalize()
    }

    /// Normalizes a `Selection`.
    ///
    /// Ranges are sorted by [Range::from], with overlapping ranges merged.
    fn normalize(mut self) -> Self {
        if self.ranges.len() < 2 {
            return self;
        }
        let mut primary = self.ranges[self.primary_index];
        self.ranges.sort_unstable_by_key(Range::from);

        self.ranges.dedup_by(|curr_range, prev_range| {
            if prev_range.overlaps(curr_range) {
                let new_range = curr_range.merge(*prev_range);
                if prev_range == &primary || curr_range == &primary {
                    primary = new_range;
                }
                *prev_range = new_range;
                true
            } else {
                false
            }
        });

        self.primary_index = self
            .ranges
            .iter()
            .position(|&range| range == primary)
            .unwrap();

        self
    }

    pub fn push(&self, range: Range) -> Self {
        let mut new = self.clone();
        new.ranges.push(range);
        new.primary_index = self.ranges.len();
        new.normalize()
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

pub fn cursor_at_byte(rope: &Rope, byte: usize) -> Cursor {
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
/// NOTE: This does not go past the LAST grapheme on the LAST line
fn byte_offset_at_cursor(rope: &Rope, cursor: &Cursor, mode: &Mode) -> usize {
    let mut offset = rope.byte_of_line(cursor.y);
    let mut col = 0;
    let mut cursor_is_past_last_grapheme = true;

    for g in rope.line(cursor.y).graphemes() {
        if col == cursor.x {
            cursor_is_past_last_grapheme = false;
            // In select mode, we want to include
            // the current cursor's grapheme
            if mode == &Mode::Select {
                offset += g.len();
            }
            break;
        }
        col += graphemes::width(&g);
        offset += g.len();
    }

    // In select mode the cursor can go after the last grapheme
    // just like insert mode. This indicates that the selection
    // includes the line ending, so we need to include that byte
    // in the range too
    let include_eol = mode == &Mode::Select;
    let is_last_line = cursor.y == rope.line_len() - 1;
    if cursor_is_past_last_grapheme && include_eol && !is_last_line {
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

        assert_eq!(one.min(one), one);
        assert_eq!(one.min(two), two);
        assert_eq!(one.min(three), one);
        assert_eq!(one.max(one), one);
        assert_eq!(one.max(two), one);
        assert_eq!(one.max(three), three);
    }

    #[test]
    fn test_overlaps() {
        fn overlaps(a: (Cursor, Cursor), b: (Cursor, Cursor)) -> bool {
            Range {head: a.0, anchor: a.1, sticky_x: 0}.overlaps(&Range {head: b.0, anchor: b.1, sticky_x: 0})
        }

        // Two non-zero-width ranges, no overlap.
        assert!(!overlaps((Cursor {x: 0, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 4, y: 0}, Cursor {x: 6, y: 0})));
        assert!(!overlaps((Cursor {x: 0, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 6, y: 0}, Cursor {x: 4, y: 0})));
        assert!(!overlaps((Cursor {x: 3, y: 0}, Cursor {x: 0, y: 0}), (Cursor {x: 4, y: 0}, Cursor {x: 6, y: 0})));
        assert!(!overlaps((Cursor {x: 3, y: 0}, Cursor {x: 0, y: 0}), (Cursor {x: 6, y: 0}, Cursor {x: 4, y: 0})));
        assert!(!overlaps((Cursor {x: 3, y: 0}, Cursor {x: 6, y: 0}), (Cursor {x: 0, y: 0}, Cursor {x: 2, y: 0})));
        assert!(!overlaps((Cursor {x: 3, y: 0}, Cursor {x: 6, y: 0}), (Cursor {x: 2, y: 0}, Cursor {x: 0, y: 0})));
        assert!(!overlaps((Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 0, y: 0}, Cursor {x: 2, y: 0})));
        assert!(!overlaps((Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 2, y: 0}, Cursor {x: 0, y: 0})));

        assert!(!overlaps((Cursor {x: 6, y: 1}, Cursor {x: 3, y: 1}), (Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0})));

        // Two non-zero-width ranges, overlap.
        assert!(overlaps((Cursor {x: 0, y: 0}, Cursor {x: 4, y: 0}), (Cursor {x: 3, y: 0}, Cursor {x: 6, y: 0})));
        assert!(overlaps((Cursor {x: 0, y: 0}, Cursor {x: 4, y: 0}), (Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0})));
        assert!(overlaps((Cursor {x: 4, y: 0}, Cursor {x: 0, y: 0}), (Cursor {x: 3, y: 0}, Cursor {x: 6, y: 0})));
        assert!(overlaps((Cursor {x: 4, y: 0}, Cursor {x: 0, y: 0}), (Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0})));
        assert!(overlaps((Cursor {x: 3, y: 0}, Cursor {x: 6, y: 0}), (Cursor {x: 0, y: 0}, Cursor {x: 4, y: 0})));
        assert!(overlaps((Cursor {x: 3, y: 0}, Cursor {x: 6, y: 0}), (Cursor {x: 4, y: 0}, Cursor {x: 0, y: 0})));
        assert!(overlaps((Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 0, y: 0}, Cursor {x: 4, y: 0})));
        assert!(overlaps((Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 4, y: 0}, Cursor {x: 0, y: 0})));

        assert!(overlaps((Cursor {x: 6, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 4, y: 0}, Cursor {x: 0, y: 1})));

        // Zero-width and non-zero-width range, no overlap.
        assert!(!overlaps((Cursor {x: 0, y: 0}, Cursor {x: 2, y: 0}), (Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0})));
        assert!(!overlaps((Cursor {x: 2, y: 0}, Cursor {x: 0, y: 0}), (Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0})));
        assert!(!overlaps((Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 0, y: 0}, Cursor {x: 2, y: 0})));
        assert!(!overlaps((Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 2, y: 0}, Cursor {x: 0, y: 0})));

        // Zero-width and non-zero-width range, overlap.
        assert!(overlaps((Cursor {x: 1, y: 0}, Cursor {x: 4, y: 0}), (Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0})));
        assert!(overlaps((Cursor {x: 4, y: 0}, Cursor {x: 1, y: 0}), (Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0})));
        assert!(overlaps((Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0}), (Cursor {x: 1, y: 0}, Cursor {x: 4, y: 0})));
        assert!(overlaps((Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0}), (Cursor {x: 4, y: 0}, Cursor {x: 1, y: 0})));

        assert!(overlaps((Cursor {x: 1, y: 0}, Cursor {x: 4, y: 0}), (Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0})));
        assert!(overlaps((Cursor {x: 4, y: 0}, Cursor {x: 1, y: 0}), (Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0})));
        assert!(overlaps((Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 1, y: 0}, Cursor {x: 4, y: 0})));
        assert!(overlaps((Cursor {x: 3, y: 0}, Cursor {x: 3, y: 0}), (Cursor {x: 4, y: 0}, Cursor {x: 1, y: 0})));

        // Two zero-width ranges, no overlap.
        assert!(!overlaps((Cursor {x: 0, y: 0}, Cursor {x: 0, y: 0}), (Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0})));
        assert!(!overlaps((Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0}), (Cursor {x: 0, y: 0}, Cursor {x: 0, y: 0})));

        // Two zero-width ranges, overlap.
        assert!(overlaps((Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0}), (Cursor {x: 1, y: 0}, Cursor {x: 1, y: 0})));
    }
}
