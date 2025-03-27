// Unicode underlined characters for stealing
// A̲ B̲ C̲ D̲ E̲ F̲ G̲ H̲ I̲ J̲ K̲ L̲ M̲ N̲ O̲ P̲ Q̲ R̲ S̲ T̲ U̲ V̲ W̲ X̲ Y̲ Z̲
use crossterm::event::{KeyCode, KeyEvent};

use crate::graphemes;

use super::Rect;
use super::theme::THEME;
use super::style::Style;
use super::buffer::Buffer;
use super::break_into_lines;
use super::borders::{Borders, Stroke};
use super::border_box::BorderBox;

pub trait ModalButtons: Default + Eq + PartialEq {
    fn to_index(&self) -> u8;
    fn from_index(index: u8) -> Self;
    fn from_key_code(code: KeyCode) -> Option<Self> where Self: std::marker::Sized;
    fn buttons(&self) -> &[Self] where Self: std::marker::Sized;
    fn text(&self) -> &'static str;
}

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub enum YesNoCancel {
    #[default]
    Yes,
    No,
    Cancel,
}

impl ModalButtons for YesNoCancel {
    fn to_index(&self) -> u8 {
        match self {
            Self::Yes => 0,
            Self::No => 1,
            Self::Cancel => 2,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Yes,
            1 => Self::No,
            _ => Self::Cancel,
        }
    }

    fn from_key_code(code: KeyCode) -> Option<Self> {
        match code {
            KeyCode::Char('y') => {
                Some(Self::Yes)
            }
            KeyCode::Char('n') => {
                Some(Self::No)
            }
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => {
                Some(Self::Cancel)
            }
            _ => None
        }
    }

    fn buttons(&self) -> &[Self] {
        &[
            Self::Yes,
            Self::No,
            Self::Cancel
        ]
    }

    fn text(&self) -> &'static str {
        match self {
            Self::Yes => " Y̲es ",
            Self::No => " N̲o ",
            Self::Cancel => " C̲ancel ",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub struct Okay;

impl ModalButtons for Okay {
    fn to_index(&self) -> u8 {
        0
    }

    fn from_index(_index: u8) -> Self {
        Self {}
    }

    fn from_key_code(code: KeyCode) -> Option<Self> where Self: std::marker::Sized {
        match code {
            KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('q') => {
                Some(Self {})
            }
            _ => None
        }
    }

    fn buttons(&self) -> &[Self] where Self: std::marker::Sized {
        &[Self {}]
    }

    fn text(&self) -> &'static str {
        " O̲K "
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub enum FileReload {
    #[default]
    Ok,
    Reload,
}

impl ModalButtons for FileReload {
    fn to_index(&self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Reload => 1,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Ok,
            _ => Self::Reload,
        }
    }

    fn from_key_code(code: KeyCode) -> Option<Self> {
        match code {
            KeyCode::Esc | KeyCode::Char('o') | KeyCode::Char('q') => {
                Some(Self::Ok)
            }
            KeyCode::Char('r') => {
                Some(Self::Reload)
            }
            _ => None
        }
    }

    fn buttons(&self) -> &[Self] {
        &[
            Self::Ok,
            Self::Reload,
        ]
    }

    fn text(&self) -> &'static str {
        match self {
            Self::Ok => " O̲K ",
            Self::Reload => " R̲eload file",
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Default)]
pub enum FileOverwrite {
    #[default]
    Overwrite,
    Reload,
    Cancel,
}

impl ModalButtons for FileOverwrite {
    fn to_index(&self) -> u8 {
        match self {
            Self::Overwrite => 0,
            Self::Reload => 1,
            Self::Cancel => 2,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            0 => Self::Overwrite,
            1 => Self::Reload,
            _ => Self::Cancel,
        }
    }

    fn from_key_code(code: KeyCode) -> Option<Self> where Self: std::marker::Sized {
        match code {
            KeyCode::Char('o') => {
                Some(Self::Overwrite)
            },
            KeyCode::Char('r') => {
                Some(Self::Reload)
            },
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => {
                Some(Self::Cancel)
            }
            _ => None
        }
    }

    fn buttons(&self) -> &[Self] {
        &[
            Self::Overwrite,
            Self::Reload,
            Self::Cancel,
        ]
    }

    fn text(&self) -> &'static str {
        match self {
            Self::Overwrite => " O̲verwrite ",
            Self::Reload => " R̲eload file ",
            Self::Cancel => " C̲ancel ",
        }
    }
}

pub struct Modal<C = YesNoCancel> {
    pub title: String,
    pub body: String,
    pub choice: C,
    pub style: Style,
}

impl<C: ModalButtons> Modal<C> {
    pub fn new(title: String, body: String) -> Self {
        Self {
            title,
            body,
            choice: C::default(),
            style: THEME.get("warning"),
        }
    }

    // Renders a centered box in the given area with a title,
    // body and a style and returns the inner area.
    // Buttons are not rendered
    fn render_box(&self, area: Rect, buffer: &mut Buffer) -> Rect {
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

    pub fn render(&self, area: Rect, buffer: &mut Buffer) {
        let inner = self.render_box(area, buffer);

        let mut x = inner.left() + 1;
        let y = inner.bottom().saturating_sub(1);

        for button in self.choice.buttons().iter() {
            let style = if &self.choice == button {
                THEME.get("ui.button.selected")
            } else {
                THEME.get("ui.button")
            };

            buffer.put_str(button.text(), x, y, style);
            x += graphemes::width(button.text()) as u16;
        }
    }

    pub fn handle_choice(&mut self, event: KeyEvent) -> bool {
        match event.code {
            KeyCode::Char('l') | KeyCode::Right => {
                let index = self.choice.to_index();
                let len = self.choice.buttons().len() as u8;
                self.choice = C::from_index((index + 1) % len);
                false
            },
            KeyCode::Char('h') | KeyCode::Left => {
                let index = self.choice.to_index();
                let len = self.choice.buttons().len() as u8;
                self.choice = C::from_index((index + len.saturating_sub(1)) % len);
                false
            }
            KeyCode::Enter => true,
            code => {
                if let Some(choice) = C::from_key_code(code) {
                    self.choice = choice;
                    true
                } else {
                    false
                }
            }
        }
    }
}
