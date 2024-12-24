use std::ops::Range;

use crop::Rope;

use crate::{document::StyleIter, graphemes::{self, GraphemeCategory}, language::syntax::HighlightEvent, selection::Selection, ui::{buffer::Buffer, theme::THEME, Position, Rect}};

fn adjust_scroll(dimension: usize, doc_cursor: usize, offset: usize, scroll: usize) -> Option<usize> {
    if doc_cursor > dimension.saturating_sub(offset + 1) + scroll {
        return Some(doc_cursor.saturating_sub(dimension.saturating_sub(offset + 1)));
    }

    if doc_cursor < scroll + offset {
        return Some(doc_cursor.saturating_sub(offset));
    }

    None
}

#[derive(Default, Debug)]
pub struct ScrollView {
    // The visual position of a cursor on the screen
    // relative to the origin 0,0 at the top left of
    // the editor (not the current view)
    pub cursor: Position,
    pub offset_x: usize,
    pub offset_y: usize,
    pub scroll_x: usize,
    pub scroll_y: usize,
}

impl ScrollView {
    pub fn ensure_cursor_is_in_view(&mut self, selection: &Selection, area: Rect) {
        if let Some(s) = adjust_scroll(area.height as usize, selection.head.y, self.offset_y, self.scroll_y) {
            self.scroll_y = s;
        }

        if let Some(s) = adjust_scroll(area.width as usize, selection.head.x, self.offset_x, self.scroll_x) {
            self.scroll_x = s;
        }

        // adjust cursor
        self.cursor.row = area.top() + selection.head.y.saturating_sub(self.scroll_y) as u16;
        self.cursor.col = area.left() + selection.head.x.saturating_sub(self.scroll_x) as u16;
    }

    pub fn render(
        &mut self,
        area: Rect,
        buffer: &mut Buffer,
        rope: &Rope,
        highlight_iter: impl Iterator<Item = HighlightEvent>,
        render_trailing_whitespace: bool,
    ) {
        let mut styles = StyleIter::new(highlight_iter);
        let (mut style, mut highlight_until) = styles.next()
            .unwrap_or((THEME.get("text"), usize::MAX));

        // loop through each visible line
        for row in self.scroll_y..self.scroll_y + area.height as usize {
            if row >= rope.line_len() { break }

            let mut offset = rope.byte_of_line(row);
            // at the start of each line we have to check if the byte offset
            // is more than the current highlight_until (accounting for new lines)
            while offset > highlight_until {
                match styles.next() {
                    Some((s, h)) => (style, highlight_until) = (s, h),
                    None => break
                }
            }

            let line = rope.line(row);
            let mut graphemes = line.graphemes();
            // accounts for multi-width graphemes
            let mut skip_next_n_cols = 0;

            // advance the iterator to account for scroll
            let mut advance = 0;
            while advance < self.scroll_x {
                if let Some(g) = graphemes.next() {
                    offset += g.len();
                    advance += graphemes::width(&g);
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
                        let width = graphemes::width(&g);
                        let x = col.saturating_sub(self.scroll_x) as u16 + area.left();

                        skip_next_n_cols = width - 1;

                        offset += g.len();

                        while offset > highlight_until {
                            match styles.next() {
                                Some((s, h)) => (style, highlight_until) = (s, h),
                                None => break
                            }
                        }

                        buffer.put_symbol(&g, x, y, style);

                        if GraphemeCategory::from(&g) == GraphemeCategory::Whitespace {
                            trailing_whitespace.push(x);
                        } else {
                            trailing_whitespace.drain(..);
                        }
                    }
                }
            }

            if render_trailing_whitespace {
                for x in trailing_whitespace {
                    // render trailing whitespace
                    buffer.put_symbol("~", x, y, THEME.get("text.whitespace"));
                }
            }
        }
    }

    pub fn visible_byte_range(&self, rope: &Rope, height: u16) -> Range<usize> {
        let from = self.scroll_y;
        let to = (from + height.saturating_sub(1) as usize).min(rope.line_len().saturating_sub(1));
        let start = rope.byte_of_line(from);
        let end = rope.byte_of_line(to + 1);

        start..end
    }

    // This needs to work with transactions
    // pub fn insert_str_at_cursor(&mut self, rope: &mut Rope, str: &str, selection: &Selection, _mode: &Mode) {
    //     let offset = self.byte_offset_at_cursor(rope, selection.head.x, selection.head.y);
    //     rope.insert(offset, str);
    //     // TODO: Move the cursor
    // }
}
