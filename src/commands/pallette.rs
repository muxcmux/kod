use crop::Rope;
use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}, style::Color};
use crate::{components::scroll_view::ScrollView, compositor::{Component, Compositor, Context, EventResult}, editable_text::EditableText, editor::Mode, ui::{border_box::BorderBox, borders::{BorderType, Borders}, buffer::Buffer, Position, Rect}};

use super::{Command, COMMANDS};

pub struct Pallette {
    input: TextInput,
    index: usize,
}

impl Pallette {
    pub fn new() -> Self {
        Self { input: TextInput::new(Rope::from("\n")), index: 0 }
    }

    fn run(&mut self, ctx: &mut Context) -> EventResult {
        let idx = self.index;

        let close = Box::new(|compositor: &mut Compositor, _: &mut Context| { compositor.pop(); });

        if let Some(cmd) = self.commands().get(idx) {
            let mut ctx = crate::commands::Context {
                editor: ctx.editor,
                compositor_callbacks: vec![],
                on_next_key_callback: None,
            };

            (cmd.func)(&mut ctx);

            if ctx.compositor_callbacks.is_empty() {
                return EventResult::Consumed(Some(close))
            }

            return EventResult::Consumed(Some(Box::new(|compositor, cx| {
                // close the pallette first
                compositor.pop();
                for cb in ctx.compositor_callbacks {
                    cb(compositor, cx);
                }
            })));
        }

        EventResult::Ignored(None)
    }

    fn commands(&mut self) -> Vec<&Command> {
        let text = self.input.text.rope.to_string();
        let text = text.trim();
        COMMANDS.iter()
            .filter(|c| {
                text == "\n" ||
                    c.name.contains(text) ||
                    c.aliases.iter().any(|c| *c == text)
            })
            .collect()
    }
}

impl Component for Pallette {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        let size = area.clip_bottom(1).centered(50, 10);

        let bbox = BorderBox::new(size)
            .title("Command")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        bbox.render(buffer).split_horizontally(2, buffer);

        let inner = bbox.inner();
        let input_size = inner.clip_bottom(inner.height.saturating_sub(1));

        self.input.render(input_size, buffer);

        // render list
        let index = self.index;
        for (i, cmd) in self.commands().iter().enumerate() {
            let (fg, caret) = if i == index {
                (Color::White, " ")
            } else {
                (Color::DarkGrey, "  ")
            };
            let y = inner.top() + (2 + i) as u16;
            buffer.put_str(caret, inner.left(), y, fg, Color::Reset);
            buffer.put_str(cmd.name, inner.left() + 2, y, fg, Color::Reset);
            buffer.put_str(cmd.desc, inner.right().saturating_sub(cmd.desc.chars().count() as u16), y, fg, Color::Reset);
        }

    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Enter =>  {
                self.run(ctx)
            },
            KeyCode::Up => {
                self.index = self.index.saturating_sub(1);
                EventResult::Consumed(None)
            },
            KeyCode::Down => {
                self.index = (self.index + 1).min(COMMANDS.len().saturating_sub(1));
                EventResult::Consumed(None)
            },
            // scroll by a page
            // KeyCode::PageUp => todo!(),
            // KeyCode::PageDown => todo!(),
            KeyCode::Esc => {
                EventResult::Consumed(Some(Box::new(|compositor, _| {
                    compositor.pop();
                })))
            },
            _ => {
                self.input.handle_key_event(event);
                self.index = 0;
                EventResult::Consumed(None)
            }
        }
    }

    fn cursor(&self, _area: Rect, _ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        (
            Some(self.input.view.cursor_position), Some(SetCursorStyle::SteadyBar)
        )
    }
}

struct TextInput {
    view: ScrollView,
    text: EditableText,
}

impl TextInput {
    fn new(rope: Rope) -> Self {
        Self {
            view: ScrollView::default(),
            text: EditableText::new(rope)
        }
    }

    fn render(&mut self, area: Rect, buffer: &mut Buffer) {
        self.view.render(area, buffer, &self.text, |_, _| {});
    }

    fn handle_key_event(&mut self, event: KeyEvent) {
        match event.code {
            KeyCode::Left      => { self.text.cursor_left(&Mode::Insert); }
            KeyCode::Right     => { self.text.cursor_right(&Mode::Insert); }
            KeyCode::Up        => { self.text.cursor_up(&Mode::Insert); }
            KeyCode::Down      => { self.text.cursor_down(&Mode::Insert); }
            KeyCode::Home      => { self.text.move_cursor_to(Some(0), None, &Mode::Insert); }
            KeyCode::End       => { self.text.move_cursor_to(Some(usize::MAX), None, &Mode::Insert); }
            KeyCode::Backspace => { self.text.delete_to_the_left(&Mode::Insert); }
            KeyCode::Char(c)   => {
                self.text.insert_char_at_cursor(c, &Mode::Insert);
            }
            _ => {}
        }
    }
}
