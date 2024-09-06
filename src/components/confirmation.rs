use crate::ui::border_box::BorderBox;
use crate::ui::borders::{BorderType, Borders};
use crate::ui::buffer::Buffer;
use crate::{compositor::{Component, Compositor, Context, EventResult}, ui::Rect};
use crossterm::event::{KeyCode, KeyEvent};
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
    pub fn new(title: String, text: String, border_type: BorderType, yes: Callback, no: Callback) -> Self {
        Self { title, text, yes, no, borders: Borders::ALL, border_type }
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
    fn render(&mut self, area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        let width = self.title_width()
            .max(PROMPT_WIDTH)
            .max(self.text_width())
            + u16::from(self.borders.intersects(Borders::LEFT))
            + u16::from(self.borders.intersects(Borders::RIGHT))
            .min(area.width);
        let height = 2 + u16::from(self.borders.intersects(Borders::TOP))
            + u16::from(self.borders.intersects(Borders::BOTTOM))
            .min(area.height);

        let area = area.centered(width, height);

        let bbox = BorderBox::new(area)
            .title(&self.title)
            .borders(self.borders)
            .border_type(self.border_type);

        bbox.render(buffer);

        let x = area.left() + u16::from(self.borders.intersects(Borders::LEFT));
        buffer.put_str(&self.title, x, area.top(), Color::White, Color::Reset);
        buffer.put_str(&self.text, x, area.top() + 1, Color::White, Color::Reset);
        buffer.put_str(PROMPT, x, area.top() + 2, Color::White, Color::Reset);
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        let cb = Box::new(|compositor: &mut Compositor, _: &mut Context| {
            _ = compositor.pop();
        });
        match event.code {
            KeyCode::Char('y') => {
                (self.yes)(ctx);
                EventResult::Consumed(Some(cb))
            },
            KeyCode::Char('n') => {
                (self.no)(ctx);
                EventResult::Consumed(Some(cb))
            },
            KeyCode::Esc | KeyCode::Char('c') => {
                EventResult::Consumed(Some(cb))
            },
            _ => EventResult::Consumed(None)
        }
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}
