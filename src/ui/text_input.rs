use crop::Rope;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::{components::scroll_view::ScrollView, editor::Mode, graphemes::{line_width, NEW_LINE, NEW_LINE_STR}, selection::Selection};

use super::{buffer::Buffer, Rect};

pub struct TextInput {
    pub rope: Rope,
    pub view: ScrollView,
    pub selection: Selection,
    pub history: Vec<String>,
    history_idx: usize,
}

impl TextInput {
    pub fn empty() -> Self {
        Self {
            rope: Rope::from(NEW_LINE_STR),
            view: ScrollView::default(),
            selection: Selection::default(),
            history: vec![],
            history_idx: 1,
        }
    }

    pub fn with_history(history: Vec<String>) -> Self {
        let history_idx = 1.max(history.len());
        Self {
            rope: Rope::from(NEW_LINE_STR),
            view: ScrollView::default(),
            selection: Selection::default(),
            history,
            history_idx,
        }
    }

    pub fn with_value(val: &str) -> Self {
        Self {
            rope: Rope::from(val),
            view: ScrollView::default(),
            selection: Selection::default(),
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
        self.move_cursor_to(Some(0), Some(0));
    }

    pub fn value(&self) -> String {
        self.rope.line(0).to_string()
    }

    pub fn render(&mut self, area: Rect, buffer: &mut Buffer) {
        self.view.ensure_cursor_is_in_view(&self.selection, area);
        self.view.render(area, buffer, &self.rope, [].into_iter(), false);
    }

    fn insert_char_at_cursor(&mut self, char: char) {
        let offset = self.selection.byte_offset_at_head(&self.rope);
        let mut buf = [0; 4];
        let text = char.encode_utf8(&mut buf);

        self.rope.insert(offset, text);

        if char == NEW_LINE {
            self.move_cursor_to(Some(0), Some(self.selection.head.y + 1));
        } else {
            self.move_cursor_to(Some(self.selection.head.x + 1), None);
        }
    }

    fn move_cursor_to(&mut self, x: Option<usize>, y: Option<usize>) {
        self.selection = self.selection.move_to(&self.rope, x, y, &Mode::Insert);
    }

    fn cursor_left(&mut self) {
        self.selection = self.selection.left(&self.rope, &Mode::Insert);
    }

    fn cursor_right(&mut self) {
        self.selection = self.selection.right(&self.rope, &Mode::Insert);
    }

    pub fn delete_to_the_left(&mut self) -> bool {
        if self.selection.head.x > 0 {
            let mut start = self.rope.byte_of_line(self.selection.head.y);
            let mut end = start;
            let idx = self.selection.grapheme_at_head(&self.rope).0 - 1;
            for (i, g) in self.rope.line(self.selection.head.y).graphemes().enumerate() {
                if i < idx { start += g.len() }
                if i == idx {
                    end = start + g.len();
                    break
                }
            }

            self.cursor_left();
            self.rope.delete(start..end);
            return true;
        } else if self.selection.head.y > 0  {
            let to = self.rope.byte_of_line(self.selection.head.y);
            let from = to.saturating_sub(NEW_LINE.len_utf8());
            // need to move cursor before deleting
            self.move_cursor_to(Some(line_width(&self.rope, self.selection.head.y - 1)), Some(self.selection.head.y - 1));
            self.rope.delete(from..to);
            return true;
        }

        false
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Left => {
                self.cursor_left();
            }
            KeyCode::Right => {
                self.cursor_right();
            }
            KeyCode::Up => {
                if let Some(value) = self.history.get(self.history_idx.saturating_sub(1)) {
                    self.rope = Rope::from(value.as_str());
                    self.move_cursor_to(Some(usize::MAX), None);
                    self.history_idx = self.history_idx.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                match self.history.get(self.history_idx + 1) {
                    Some(value) => {
                        self.rope = Rope::from(value.as_str());
                        self.move_cursor_to(Some(usize::MAX), None);
                        self.history_idx += 1;
                    }
                    None => {
                        self.clear();
                    }
                }
            }
            KeyCode::Home => {
                self.move_cursor_to(Some(0), None);
            }
            KeyCode::End => {
                self.move_cursor_to(Some(usize::MAX), None);
            }
            KeyCode::Backspace => {
                self.history_idx = self.history.len();
                self.delete_to_the_left();
            }
            KeyCode::Char(c) => {
                self.history_idx = self.history.len();
                if event.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'h' => self.move_cursor_to(Some(0), None),
                        'l' => self.move_cursor_to(Some(usize::MAX), None),
                        _ => {},
                    }
                } else {
                    self.insert_char_at_cursor(c);
                }
            }
            _ => {}
        }
    }
}
