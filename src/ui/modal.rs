use crossterm::event::{KeyCode, KeyEvent};

use crate::graphemes;

use super::{border_box::BorderBox, borders::{Borders, Stroke}, break_into_lines, buffer::Buffer, style::Style, theme::THEME, Rect};

const PROMPT_YES: &str = " Y̲es ";
const PROMPT_NO: &str = " N̲o ";
const PROMPT_CANCEL: &str = " C̲ancel ";

// Renders a centered box with a title, body and style
// Returns the inner area of the box
// Buttons are not rendered
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Choice {
    Yes,
    No,
    Cancel,
}

impl From<Choice> for u8 {
    fn from(val: Choice) -> Self {
        match val {
            Choice::Yes => 0,
            Choice::No => 1,
            Choice::Cancel => 2,
        }
    }
}

impl From<u8> for Choice {
    fn from(val: u8) -> Self {
        match val {
            0 => Choice::Yes,
            1 => Choice::No,
            _ => Choice::Cancel,
        }
    }
}

pub struct Modal {
    pub title: String,
    pub body: String,
    pub choice: Choice,
    pub style: Style,
}

impl Modal {
    pub fn new(title: String, body: String) -> Self {
        Self {
            title,
            body,
            choice: Choice::Yes,
            style: THEME.get("warning"),
        }
    }

    // pub fn style(mut self, style: Style) -> Self {
    //     self.style = style;
    //     self
    // }

    // Renders a centered box in the given area with a title,
    // body and a style and returns the inner area.
    // Buttons are not rendered
    pub fn render_box(&self, area: Rect, buffer: &mut Buffer) -> Rect {
        const PADDING: usize = 4;

        // 80% of the screen, 60 cols, body/title width, or at least 30
        let max_width = (graphemes::width(&self.body) + PADDING)
            .max(graphemes::width(&self.title) + PADDING)
            .clamp(21, 60)
            .min(area.width as usize * 8 / 10) as u16;

        let lines = break_into_lines(&self.body, max_width.saturating_sub(4) as usize);

        // This will overflow for large amounts of text (> 2^16 lines)
        let box_area = area.centered(max_width, ((lines.len() + 4) as u16).min(area.height));

        let bbox = BorderBox::new(box_area)
            .title(&self.title)
            .borders(Borders::ALL)
            .style(self.style)
            .title_style(self.style)
            .stroke(Stroke::Plain);

        bbox.render(buffer);

        let inner = bbox.inner();
        let y = inner.top();
        let x = inner.left() + 1;

        for (i, line) in lines.iter().enumerate() {
            buffer.put_str(line, x, y + i as u16, THEME.get("ui.dialog.text"));
        }

        inner
    }

    // Renders the box along with the prompt buttons in the given area
    pub fn render_all(&self, area: Rect, buffer: &mut Buffer) {
        let inner = self.render_box(area, buffer);

        let (first, second, third) = match self.choice {
            Choice::Yes => ("ui.button.selected", "ui.button", "ui.button"),
            Choice::No => ("ui.button", "ui.button.selected", "ui.button"),
            Choice::Cancel => ("ui.button", "ui.button", "ui.button.selected"),
        };

        let x = inner.left() + 1;
        let y = inner.bottom().saturating_sub(1);

        buffer.put_str(PROMPT_YES, x, y, THEME.get(first));
        let x = x + graphemes::width(PROMPT_YES) as u16;
        buffer.put_str(PROMPT_NO, x, y, THEME.get(second));
        let x = x + graphemes::width(PROMPT_NO) as u16;
        buffer.put_str(PROMPT_CANCEL, x, y, THEME.get(third));
    }

    pub fn handle_choice(&mut self, event: KeyEvent) -> bool {
        match event.code {
            KeyCode::Char('y') => {
                self.choice = Choice::Yes;
                true
            }
            KeyCode::Char('n') => {
                self.choice = Choice::No;
                true
            }
            KeyCode::Enter => true,
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => {
                self.choice = Choice::Cancel;
                true
            }
            KeyCode::Char('l') | KeyCode::Right => {
                let as_int: u8 = self.choice.into();
                self.choice = ((as_int + 1) % 3).into();
                false
            },
            KeyCode::Char('h') | KeyCode::Left => {
                let as_int: u8 = self.choice.into();
                self.choice = ((as_int + 2) % 3).into();
                false
            }
            _ => false
        }
    }
}
