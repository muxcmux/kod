use crop::Rope;
use crossterm::event::{KeyCode, KeyEvent};

use crate::{components::scroll_view::ScrollView, editor::Mode, graphemes::{NEW_LINE, NEW_LINE_STR}};

use super::{buffer::Buffer, Rect};

pub struct TextInput {
    pub rope: Rope,
    pub view: ScrollView,
    pub history: Vec<String>,
    history_idx: usize,
}

impl TextInput {
    pub fn empty() -> Self {
        Self {
            rope: Rope::from(NEW_LINE_STR),
            view: ScrollView::default(),
            history: vec![],
            history_idx: 1,
        }
    }

    pub fn with_history(history: Vec<String>) -> Self {
        let history_idx = 1.max(history.len());
        Self {
            rope: Rope::from(NEW_LINE_STR),
            view: ScrollView::default(),
            history,
            history_idx,
        }
    }

    pub fn with_value(val: &str) -> Self {
        Self {
            rope: Rope::from(val),
            view: ScrollView::default(),
            history: vec![],
            history_idx: 1,
        }
    }

    pub fn remember(&mut self) {
        let val = self.value();
        if self.history.last().is_none_or(|v| *v != val) {
            self.history.push(val);
        }
        self.history_idx = self.history.len();
    }

    pub fn clear(&mut self) {
        self.rope = Rope::from(NEW_LINE_STR);
        self.history_idx = self.history.len();
        self.view.move_cursor_to(&self.rope, Some(0), Some(0), &Mode::Insert);
    }

    pub fn value(&self) -> String {
        self.rope.line(0).to_string()
    }

    pub fn render(&mut self, area: Rect, buffer: &mut Buffer) {
        self.view.ensure_cursor_is_in_view(area);
        self.view.render(area, buffer, &self.rope, [].into_iter(), false);
    }

    fn insert_char_at_cursor(&mut self, char: char, mode: &Mode) {
        let offset = self.view.byte_offset_at_cursor(&self.rope, self.view.text_cursor_x, self.view.text_cursor_y);
        let mut buf = [0; 4];
        let text = char.encode_utf8(&mut buf);

        self.rope.insert(offset, text);

        if char == NEW_LINE {
            self.view.move_cursor_to(&self.rope, Some(0), Some(self.view.text_cursor_y + 1), mode);
        } else {
            self.view.move_cursor_to(&self.rope, Some(self.view.text_cursor_x + 1), None, mode);
        }
    }

    pub fn delete_to_the_left(&mut self, mode: &Mode) -> bool {
        if self.view.text_cursor_x > 0 {
            let mut start = self.rope.byte_of_line(self.view.text_cursor_y);
            let mut end = start;
            let idx = self.view.grapheme_at_cursor(&self.rope).0 - 1;
            for (i, g) in self.rope.line(self.view.text_cursor_y).graphemes().enumerate() {
                if i < idx { start += g.len() }
                if i == idx {
                    end = start + g.len();
                    break
                }
            }

            self.view.cursor_left(&self.rope, &Mode::Insert);
            self.rope.delete(start..end);
            return true;
        } else if self.view.text_cursor_y > 0  {
            let to = self.rope.byte_of_line(self.view.text_cursor_y);
            let from = to.saturating_sub(NEW_LINE.len_utf8());
            // need to move cursor before deleting
            self.view.move_cursor_to(&self.rope, Some(self.view.line_width(&self.rope, self.view.text_cursor_y - 1)), Some(self.view.text_cursor_y - 1), mode);
            self.rope.delete(from..to);
            return true;
        }

        false
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Left => {
                self.view.cursor_left(&self.rope, &Mode::Insert);
            }
            KeyCode::Right => {
                self.view.cursor_right(&self.rope, &Mode::Insert);
            }
            KeyCode::Up => {
                if let Some(value) = self.history.get(self.history_idx.saturating_sub(1)) {
                    self.rope = Rope::from(value.as_str());
                    self.view.move_cursor_to(&self.rope, Some(usize::MAX), None, &Mode::Insert);
                    self.history_idx = self.history_idx.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                match self.history.get(self.history_idx + 1) {
                    Some(value) => {
                        self.rope = Rope::from(value.as_str());
                        self.view.move_cursor_to(&self.rope, Some(usize::MAX), None, &Mode::Insert);
                        self.history_idx += 1;
                    }
                    None => {
                        self.clear();
                    }
                }
            }
            KeyCode::Home => {
                self.view.move_cursor_to(&self.rope, Some(0), None, &Mode::Insert);
            }
            KeyCode::End => {
                self.view.move_cursor_to(&self.rope, Some(usize::MAX), None, &Mode::Insert);
            }
            KeyCode::Backspace => {
                self.history_idx = self.history.len();
                self.delete_to_the_left(&Mode::Insert);
            }
            KeyCode::Char(c) => {
                self.history_idx = self.history.len();
                self.insert_char_at_cursor(c, &Mode::Insert);
            }
            _ => {}
        }
    }
}
