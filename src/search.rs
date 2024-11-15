use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}};

use crate::{compositor::{Component, Compositor, Context, EventResult}, current, editor::Mode, rope::RopeCursor, ui::{borders::{BOTTOM_LEFT, BOTTOM_RIGHT, HORIZONTAL, HORIZONTAL_UP, VERTICAL, VERTICAL_LEFT, VERTICAL_RIGHT}, buffer::Buffer, text_input::TextInput, theme::THEME, Position, Rect}};

#[derive(Default)]
pub struct SearchState {
    pub query_history: Vec<String>,
    pub focused: bool,
    pub total_matches: usize,
    pub current_match: usize,
}

pub struct Search {
    input: TextInput,
}

impl Search {
    pub fn new(query_history: Vec<String>) -> Self {
        Self {
            input: TextInput::with_history(query_history),
        }
    }

    pub fn with_term(term: &str) -> Self {
        Self {
            input: TextInput::with_value(term),
        }
    }
}

impl Component for Search {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        buffer.clear(area.clip_top(area.height.saturating_sub(1)));

        let style = if ctx.editor.search.focused {
            "ui.text_input"
        } else {
            "ui.text_input.blur"
        };

        buffer.put_str("", area.left() + 1, area.bottom().saturating_sub(1), THEME.get(style));

        let y = area.bottom().saturating_sub(2);

        for i in area.left()..area.width {
            match buffer.get_symbol(i, y) {
                Some(ref s) => {
                    if [VERTICAL, BOTTOM_RIGHT, BOTTOM_LEFT, VERTICAL_LEFT, VERTICAL_RIGHT, HORIZONTAL_UP].contains(s) {
                        buffer.put_str(HORIZONTAL_UP, i, y, THEME.get("ui.pane.border"));
                    } else {
                        buffer.put_str(HORIZONTAL, i, y, THEME.get("ui.pane.border"));
                    }
                },
                None => {
                    buffer.put_str(HORIZONTAL, i, y, THEME.get("ui.pane.border"));
                },
            }
        }

        let input_size = area.clip_top(area.height.saturating_sub(1)).clip_left(4);

        if ctx.editor.search.focused {
            self.input.render(input_size, buffer);
        } else {
            buffer.put_str(&self.input.value(), area.left() + 4, area.bottom().saturating_sub(1), THEME.get("ui.text_input.blur"));
        }

        if ctx.editor.search.total_matches > 0 {
            let label = format!("Match {} of {}", ctx.editor.search.current_match + 1, ctx.editor.search.total_matches);
            let label_len = label.chars().count();
            buffer.put_str(&label, area.right().saturating_sub(1 + label_len as u16), area.bottom().saturating_sub(1), THEME.get("ui.text_input.blur"));
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        let close = Box::new(|comp: &mut Compositor, _: &mut Context| {
            comp.pop();
        });

        if !ctx.editor.search.focused {
            return match event.code {
                KeyCode::Esc => {
                    if ctx.editor.mode != Mode::Normal {
                        EventResult::Ignored(Some(close))
                    } else {
                        EventResult::Consumed(Some(close))
                    }
                }
                _ => EventResult::Ignored(None)
            }
        }

        match event.code {
            KeyCode::Esc => EventResult::Consumed(Some(close)),
            KeyCode::Enter => {
                self.input.remember();
                ctx.editor.search.query_history = self.input.history.clone();

                if search(ctx, false) {
                    EventResult::Consumed(None)
                } else {
                    EventResult::Consumed(Some(close))
                }
            }
            _ => {
                self.input.handle_key_event(event);
                EventResult::Consumed(None)
            }
        }
    }

    fn cursor(&self, _area: Rect, ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        if !ctx.editor.search.focused {
            return (None, None)
        }

        (
            Some(self.input.view.view_cursor_position),
            Some(SetCursorStyle::SteadyBar),
        )
    }
}

pub fn search(ctx: &mut Context, backwards: bool) -> bool {
    let query = ctx.editor.search.query_history.last().unwrap();
    match regex_cursor::engines::meta::Regex::new(query) {
        Ok(re) => {
            let (pane, doc) = current!(ctx.editor);

            let haystack = regex_cursor::Input::new(RopeCursor::new(&doc.rope));

            let mut matches: Vec<_> = re.find_iter(haystack).collect();
            matches.sort_by_key(|a| a.start());

            if matches.is_empty() {
                ctx.editor.set_warning(format!("No matches found for {}", query));
            } else {
                let offset = pane.view.byte_offset_at_cursor(&doc.rope, pane.view.text_cursor_x, pane.view.text_cursor_y);

                ctx.editor.search.total_matches = matches.len();

                if backwards {
                    ctx.editor.search.current_match = matches.len() - 1;
                    for (i, m) in matches.iter().enumerate().rev() {
                        if m.start() < offset {
                            ctx.editor.search.current_match = i;
                            break;
                        }
                    }
                } else {
                    ctx.editor.search.current_match = 0;
                    for (i, m) in matches.iter().enumerate() {
                        if m.start() > offset {
                            ctx.editor.search.current_match = i;
                            break;
                        }
                    }
                }

                let (x, y) = pane.view.cursor_at_byte(&doc.rope, matches[ctx.editor.search.current_match].start());
                pane.view.move_cursor_to(&doc.rope, Some(x), Some(y), &ctx.editor.mode);

                ctx.editor.search.focused = false;

                return true;
            }
        },
        Err(_) => {
            ctx.editor.set_error("Invalid search regex");
        },
    }

    false
}
