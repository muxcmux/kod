use crate::ui::buffer::Buffer;
use crate::ui::modal::Modal;
use crate::ui::theme::THEME;
use crate::{compositor::{Component, Context, EventResult}, ui::Rect};
use crossterm::event::KeyEvent;

pub struct Alert {
    modal: Modal
}

impl Alert {
    pub fn new(title: String, body: String) -> Self {
        let modal = Modal::new(title, body);
        Self { modal }
    }
}

impl Component for Alert {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        let inner = self.modal.render_box(area, buffer);

        buffer.put_str(" OK ", inner.left() + 1, inner.bottom().saturating_sub(1), THEME.get("ui.button.selected"))
    }

    fn handle_key_event(&mut self, _event: KeyEvent, _ctx: &mut Context) -> EventResult {
        self.dismiss()
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}
