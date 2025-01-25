use crate::document::{Document, DocumentId};
use crate::graphemes;
use crate::ui::border_box::BorderBox;
use crate::ui::borders::{Stroke, Borders};
use crate::ui::buffer::Buffer;
use crate::ui::theme::THEME;
use crate::{compositor::{Component, Compositor, Context, EventResult}, ui::Rect};
use crossterm::event::{KeyCode, KeyEvent};

fn doc<'c>(ctx: &'c mut Context, ignored: &[DocumentId]) -> Option<(&'c DocumentId, &'c Document)> {
    ctx.editor.documents
        .iter()
        .find(|(id, doc)| doc.is_modified() && !ignored.contains(id))
}

fn render_dialog(choice: u8, doc: &Document, area: Rect, buffer: &mut Buffer) {
    let text = format!(" Save changes to {}? ", doc.filename_display());
    let text_width = graphemes::width(&text) as u16;

    let width = TITLE_WIDTH
        .max(PROMPT_WIDTH)
        .max(text_width)
        + 1 // for left border
        + 1 // for right border
        .min(area.width);
    let height = 3
        + 1 // for bottom border
        + 1 // for top border
        .min(area.height);

    let area = area.centered(width, height);

    let bbox = BorderBox::new(area)
        .title(TITLE)
        .borders(Borders::ALL)
        .stroke(Stroke::Plain);

    bbox.render(buffer);

    let x = area.left() + 1;
    buffer.put_str(&text, x, area.top() + 1, THEME.get("ui.dialog.text"));

    let (first, second, third) = match choice {
        0 => ("ui.button.selected", "ui.button", "ui.button"),
        1 => ("ui.button", "ui.button.selected", "ui.button"),
        _ => ("ui.button", "ui.button", "ui.button.selected"),
    };

    let x = x + 1;
    let y = area.top() + 3;

    buffer.put_str(PROMPT_YES, x, y, THEME.get(first));
    let x = x + PROMPT_YES.len() as u16;
    buffer.put_str(PROMPT_NO, x, y, THEME.get(second));
    let x = x + PROMPT_NO.len() as u16;
    buffer.put_str(PROMPT_CANCEL, x, y, THEME.get(third));
}

const TITLE: &str = "Exit";
const TITLE_WIDTH: u16 = 4;
const PROMPT_YES: &str = " Yes ";
const PROMPT_NO: &str = " No ";
const PROMPT_CANCEL: &str = " Cancel ";
const PROMPT_WIDTH: u16 = 19;

pub struct Dialog {
    choice: u8,
    ignored_docs: Vec<DocumentId>,
}

impl Dialog {
    pub fn new() -> Self {
        Self { choice: 0, ignored_docs: vec![] }
    }

    fn ignore(ignored_docs: Vec<DocumentId>) -> Self {
        Self { choice: 0, ignored_docs }
    }

    fn yes(&mut self, ctx: &mut Context) -> EventResult {
        if let Some((id, _)) = doc(ctx, &self.ignored_docs) {
            let id = *id;
            ctx.editor.save_document(id);
        }
        let ignored = self.ignored_docs.clone();
        EventResult::Consumed(Some(Box::new(move |compositor: &mut Compositor, c: &mut Context| {
            _ = compositor.pop();
            if doc(c, &ignored).is_some() {
                compositor.push(Box::new(Dialog::new()))
            } else {
                c.editor.quit();
            }
        })))
    }

    fn no(&mut self, ctx: &mut Context) -> EventResult {
        let mut ignored = self.ignored_docs.clone();
        if let Some((id, _)) = doc(ctx, &self.ignored_docs) {
            let id = *id;
            ignored.push(id);
        }
        EventResult::Consumed(Some(Box::new(move |compositor: &mut Compositor, c: &mut Context| {
            _ = compositor.pop();
            if doc(c, &ignored).is_some() {
                compositor.push(Box::new(Dialog::ignore(ignored)));
            } else {
                c.editor.quit();
            }
        })))
    }
}

impl Component for Dialog {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let (_, doc) = doc(ctx, &self.ignored_docs)
            .expect("Rendering the save confirmation dialog without unsaved docs shouldn't happen");
        render_dialog(self.choice, doc, area, buffer);
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Char('y') => self.yes(ctx),
            KeyCode::Char('n') => self.no(ctx),
            KeyCode::Esc | KeyCode::Char('c') => self.dismiss(),
            KeyCode::Enter => {
                match self.choice {
                    0 => self.yes(ctx),
                    1 => self.no(ctx),
                    _ => self.dismiss()
                }
            },
            KeyCode::Char('l') | KeyCode::Right => {
                self.choice = (self.choice + 1) % 3;
                EventResult::Consumed(None)
            },
            KeyCode::Char('h') | KeyCode::Left => {
                self.choice = (self.choice + 2 ) % 3;
                EventResult::Consumed(None)
            }
            _ => EventResult::Consumed(None)
        }
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}
