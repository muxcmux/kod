use crossterm::style::Color;

use crate::{editable_text::{EditableText, GraphemeCategory}, ui::{buffer::Buffer, Position, Rect}};

fn adjust_scroll(dimension: usize, doc_cursor: usize, offset: usize, scroll: usize) -> Option<usize> {
    if doc_cursor > dimension.saturating_sub(offset + 1) + scroll {
        return Some(doc_cursor.saturating_sub(dimension.saturating_sub(offset + 1)));
    }

    if doc_cursor < scroll + offset {
        return Some(doc_cursor.saturating_sub(offset));
    }

    None
}

#[derive(Default)]
pub struct ScrollView {
    pub cursor_position: Position,
    pub offset_x: usize,
    pub offset_y: usize,
    pub scroll_x: usize,
    pub scroll_y: usize,
}

impl ScrollView {
    fn ensure_cursor_is_in_view(&mut self, area: Rect, text: &EditableText) {
        if let Some(s) = adjust_scroll(area.height as usize, text.cursor_y, self.offset_y, self.scroll_y) {
            self.scroll_y = s;
        }

        if let Some(s) = adjust_scroll(area.width as usize, text.cursor_x, self.offset_x, self.scroll_x) {
            self.scroll_x = s;
        }

        // adjust cursor
        self.cursor_position.y = area.top() + text.cursor_y.saturating_sub(self.scroll_y) as u16;
        self.cursor_position.x = area.left() + text.cursor_x.saturating_sub(self.scroll_x) as u16;
    }

    pub fn render<F>(&mut self, area: Rect, buffer: &mut Buffer, text: &EditableText, mut ws_callback: F)
        where F: FnMut(&mut Buffer, (u16, u16))
    {
        self.ensure_cursor_is_in_view(area, text);

        for row in self.scroll_y..self.scroll_y + area.height as usize {
            if row >= text.lines_len() {
                break;
            }
            let line = text.rope.line(row);
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

                        if matches!(GraphemeCategory::from(&g), GraphemeCategory::Whitespace) {
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
}
