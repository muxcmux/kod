use crate::graphemes;
use crate::ui::border_box::BorderBox;
use crate::ui::borders::{Stroke, Borders};
use crate::ui::buffer::Buffer;
use crate::ui::theme::THEME;
use crate::{compositor::{Component, Context, EventResult}, ui::Rect};
use crossterm::event::KeyEvent;

pub struct Alert {
    title: String,
    body: String,
}

impl Alert {
    pub fn new(title: String, body: String) -> Self {
        Self { title, body }
    }
}

fn break_into_lines(string: &str, max_width: usize) -> Vec<String> {
    let width = graphemes::width(string);
    if width <= max_width {
        return vec![string.to_string()]
    }

    let mut line_width = 0;
    let mut line = String::new();
    let mut lines = Vec::with_capacity(width / max_width + 1);

    for word in string.split(' ') {
        let width = graphemes::width(word).max(1);
        if line_width + width <= max_width {
            line.push_str(word);
            line.push(' ');
            line_width += width + 1
        } else {
            lines.push(line);
            line = word.to_string();
            line.push(' ');
            line_width = width + 1;
        }
    }

    lines.push(line);

    lines
}

impl Component for Alert {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        // 80% of the screen, 50 cols, or the body/title size
        let max_width = 55.min(area.width as usize * 8 / 10)
            .min((graphemes::width(&self.body) + 4).max(graphemes::width(&self.title) + 4)) as u16;

        let lines = break_into_lines(&self.body, max_width.saturating_sub(4) as usize);

        // This will overflow for large amounts of text (> 2^16 lines)
        let box_area = area.centered(max_width, (lines.len() + 4) as u16);

        let bbox = BorderBox::new(box_area)
            .title(&self.title)
            .borders(Borders::ALL)
            .style(THEME.get("warning"))
            .title_style(THEME.get("warning"))
            .stroke(Stroke::Plain);

        bbox.render(buffer);

        let inner = bbox.inner();
        let y = inner.top();
        let x = inner.left() + 1;

        for (i, line) in lines.iter().enumerate() {
            buffer.put_str(line, x, y + i as u16, THEME.get("ui.dialog.text"));
        }

        buffer.put_str(" OK ", inner.left() + 1, inner.bottom().saturating_sub(1), THEME.get("ui.button.selected"))
    }

    fn handle_key_event(&mut self, _event: KeyEvent, _ctx: &mut Context) -> EventResult {
        self.dismiss()
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}
