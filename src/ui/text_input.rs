use crop::Rope;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use crate::textobject::{Words, WordsBackwards};
use crate::{editor::Mode, selection::Range};
use crate::graphemes::{self, NEW_LINE_STR, NEW_LINE_STR_WIN};

use super::{buffer::Buffer, scroll::Scroll, style::Style, theme::THEME, Rect};

pub struct TextInput {
    pub rope: Rope,
    pub scroll: Scroll,
    pub cursor: Range,
}

impl TextInput {
    pub fn empty() -> Self {
        Self {
            rope: Rope::from(NEW_LINE_STR),
            scroll: Scroll::default(),
            cursor: Range::default(),
        }
    }

    pub fn with_value(val: &str) -> Self {
        Self {
            rope: Rope::from(val),
            scroll: Scroll::default(),
            cursor: Range::default(),
        }
    }

    pub fn clear(&mut self) {
        self.rope = Rope::from(NEW_LINE_STR);
        self.move_cursor_to(0);
    }

    pub fn set_value(&mut self, value: &str) {
        self.rope = Rope::from(format!("{value}\n"));
    }

    pub fn value(&self) -> String {
        self.rope.line(0).to_string()
    }

    pub fn render(&mut self, area: Rect, buffer: &mut Buffer, style: Option<Style>) {
        self.scroll.ensure_point_is_visible(self.cursor.head.x, self.cursor.head.y, &area, None);

        let mut graphemes = self.rope.line(0).graphemes();
        // accounts for multi-width graphemes
        let mut skip_next_n_cols = 0;

        // advance the iterator to account for scroll
        let mut advance = 0;
        while advance < self.scroll.x {
            if let Some(g) = graphemes.next() {
                advance += graphemes::width(&g);
                skip_next_n_cols = advance.saturating_sub(self.scroll.x);
            } else {
                break
            }
        }

        for col in self.scroll.x..self.scroll.x + area.width as usize {
            if skip_next_n_cols > 0 {
                skip_next_n_cols -= 1;
                continue;
            }
            match graphemes.next() {
                None => break,
                Some(g) => {
                    let width = graphemes::width(&g);
                    let x = col.saturating_sub(self.scroll.x) as u16 + area.left();

                    skip_next_n_cols = width - 1;

                    buffer.put_symbol(&g, x, area.top(), style.unwrap_or(THEME.get("ui.text_input")));
                }
            }
        }
    }

    pub fn move_cursor_to(&mut self, x: usize) {
        self.cursor = self.cursor.move_to(&self.rope, Some(x), None, &Mode::Insert);
    }

    fn cursor_left(&mut self) {
        self.cursor = self.cursor.left(&self.rope, &Mode::Insert);
    }

    fn cursor_right(&mut self) {
        self.cursor = self.cursor.right(&self.rope, &Mode::Insert);
    }

    fn word_left(&mut self) {
        let slice = self.rope.line(0);
        for word in WordsBackwards::new(slice) {
            if word.is_blank(slice) { continue }

            if self.cursor.head.x > word.start {
                self.move_cursor_to(word.start);
                break
            }
        }
    }

    fn word_right(&mut self) {
        let slice = self.rope.line(0);
        let mut moved = false;
        for word in Words::new(slice) {
            if word.is_blank(slice) { continue }

            if self.cursor.head.x < word.start {
                self.move_cursor_to(word.start);
                moved = true;
                break
            }
        }

        if !moved {
            self.move_cursor_to(usize::MAX);
        }
    }

    fn delete_word_left(&mut self) -> bool {
        if self.cursor.head.x > 0 {
            let slice = self.rope.line(0);
            let mut words = WordsBackwards::new(slice).peekable();
            while let Some(word) = words.next() {
                if self.cursor.head.x > word.start {
                    let end = if word.is_blank(slice) {
                        match words.peek() {
                            Some(next) => next.start,
                            None => 0,
                        }
                    } else {
                        word.start
                    };
                    let cursor = self.cursor.move_to(&self.rope, Some(end), None, &Mode::Select);
                    let byte_range = cursor.byte_range(&self.rope, &Mode::Normal);
                    self.rope.delete(byte_range);
                    self.cursor = cursor.collapse_to_head();
                    return true
                }
            }
            return true
        }

        false
    }

    pub fn delete_to_the_left(&mut self) -> bool {
        if self.cursor.head.x > 0 {
            let range = self.cursor.move_to(&self.rope, Some(self.cursor.head.x - 1), None, &Mode::Select);
            self.rope.delete(range.byte_range(&self.rope, &Mode::Insert));
            self.cursor = range.collapse_to_head();
            return true;
        }

        false
    }

    // Some(true) -> Event handled and input changed
    // Some(false) -> Event Handled and input not changed
    // None -> Event unhandled
    // This should probably be an enum...
    pub fn handle_key_event(&mut self, event: KeyEvent) -> Option<bool> {
        match event.code {
            KeyCode::Left => {
                if event.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) {
                    self.word_left();
                } else {
                    self.cursor_left();
                }
                Some(false)
            }
            KeyCode::Right => {
                if event.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) {
                    self.word_right();
                } else {
                    self.cursor_right();
                }
                Some(false)
            }
            KeyCode::Home => {
                self.move_cursor_to(0);
                Some(false)
            }
            KeyCode::End => {
                self.move_cursor_to(usize::MAX);
                Some(false)
            }
            KeyCode::Backspace => {
                if event.modifiers.contains(KeyModifiers::ALT) {
                    Some(self.delete_word_left())
                } else {
                    Some(self.delete_to_the_left())
                }
            }
            KeyCode::Char(c) => {
                if event.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'h' => {
                            self.move_cursor_to(0);
                            Some(false)
                        }
                        'l' => {
                            self.move_cursor_to(usize::MAX);
                            Some(false)
                        }
                        'w' => {
                            Some(self.delete_word_left())
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None
        }
    }

    pub fn handle_buffered_input(&mut self, string: &str) {
        let offset = self.cursor.byte_range(&self.rope, &Mode::Insert).start;
        let escaped = string.replace(NEW_LINE_STR, "\\n")
            .replace(NEW_LINE_STR_WIN, "\\n\\r");
        let width = graphemes::width(&escaped);
        self.rope.insert(offset, escaped);
        self.move_cursor_to(self.cursor.head.x + width);
    }
}
