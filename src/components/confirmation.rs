use crate::ui::borders::{BorderType, Borders};
use crate::ui::buffer::Buffer;
use crate::{compositor::{Component, Compositor, Context, EventResult}, ui::{Position, Rect}};
use crossterm::event::KeyEvent;
use crossterm::style::Color;
use unicode_segmentation::UnicodeSegmentation;

type Callback = Box<dyn FnMut(&mut Context)>;

pub struct Dialog {
    borders: Borders,
    border_type: BorderType,
    yes: Callback,
    no: Callback,
    title: String,
    text: String,
}

impl Dialog {
    pub fn new(title: String, text: String, yes: Callback, no: Callback) -> Self {
        Self { title, text, yes, no, borders: Borders::ALL, border_type: BorderType::default() }
    }

    fn title_width(&self) -> u16 {
        self.title.graphemes(true).map(|g| unicode_display_width::width(g) as u16).sum()
    }

    fn text_width(&self) -> u16 {
        self.text.graphemes(true).map(|g| unicode_display_width::width(g) as u16).sum()
    }
}

const PROMPT: &str = " [Y]es, [N]o, [C]ancel ";
const PROMPT_WIDTH: u16 = 23;

impl Component for Dialog {
    fn resize(&mut self, new_size: Rect, ctx: &mut Context) {
        //
    }

    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let width = self.title_width()
            .max(PROMPT_WIDTH)
            .max(self.text_width())
            + u16::from(self.borders.intersects(Borders::LEFT))
            + u16::from(self.borders.intersects(Borders::RIGHT))
            .min(area.width);
        let height = 2 + u16::from(self.borders.intersects(Borders::TOP))
            + u16::from(self.borders.intersects(Borders::BOTTOM))
            .min(area.height);
        let area = Rect {
            width, height,
            position: Position {
                x: area.width.saturating_sub(width) / 2,
                y: area.height.saturating_sub(height) / 2
            }
        };

        buffer.clear(area);

        let symbols = BorderType::line_symbols(self.border_type);

        // Sides
        if self.borders.intersects(Borders::LEFT) {
            for y in area.top()..area.bottom() {
                buffer.put_symbol(symbols.vertical, area.left(), y, Color::White, Color::Reset)
            }
        }
        if self.borders.intersects(Borders::TOP) {
            for x in area.left()..area.right() {
                buffer.put_symbol(symbols.horizontal, x, area.top(), Color::White, Color::Reset)
            }
        }
        if self.borders.intersects(Borders::RIGHT) {
            let x = area.right().saturating_sub(1);
            for y in area.top()..area.bottom() {
                buffer.put_symbol(symbols.vertical, x, y, Color::White, Color::Reset)
            }
        }
        if self.borders.intersects(Borders::BOTTOM) {
            let y = area.bottom().saturating_sub(1);
            for x in area.left()..area.right() {
                buffer.put_symbol(symbols.horizontal, x, y, Color::White, Color::Reset)
            }
        }

        // Corners
        if self.borders.contains(Borders::RIGHT | Borders::BOTTOM) {
            buffer.put_symbol(symbols.bottom_right, area.right().saturating_sub(1), area.bottom().saturating_sub(1), Color::White, Color::Reset)
        }
        if self.borders.contains(Borders::RIGHT | Borders::TOP) {
            buffer.put_symbol(symbols.top_right, area.right().saturating_sub(1), area.top(), Color::White, Color::Reset)
        }
        if self.borders.contains(Borders::LEFT | Borders::BOTTOM) {
            buffer.put_symbol(symbols.bottom_left, area.left(), area.bottom().saturating_sub(1), Color::White, Color::Reset)
        }
        if self.borders.contains(Borders::LEFT | Borders::TOP) {
            buffer.put_symbol(symbols.top_left, area.left(), area.top(), Color::White, Color::Reset)
        }

        let x = area.left() + u16::from(self.borders.intersects(Borders::LEFT));
        buffer.put_string(self.title.clone(), x, area.top(), Color::White, Color::Reset);
        buffer.put_string(self.text.clone(), x, area.top() + 1, Color::White, Color::Reset);
        buffer.put_string(PROMPT.to_string(), x, area.top() + 2, Color::White, Color::Reset);
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> crate::compositor::EventResult {
        let cb = Box::new(|compositor: &mut Compositor, _: &mut Context| {
            _ = compositor.pop();
        });
        match event.code {
            crossterm::event::KeyCode::Char('y') => {
                (self.yes)(ctx);
                EventResult::Consumed(Some(cb))
            },
            crossterm::event::KeyCode::Char('n') => {
                (self.no)(ctx);
                EventResult::Consumed(Some(cb))
            },
            crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('c') => {
                EventResult::Consumed(Some(cb))
            },
            _ => EventResult::Consumed(None)
        }
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}
