use crate::document::{Document, DocumentId};
use crate::ui::buffer::Buffer;
use crate::ui::modal::{Choice, Modal};
use crate::{compositor::{Component, Compositor, Context, EventResult}, ui::Rect};
use crossterm::event::KeyEvent;

fn doc<'c>(ctx: &'c mut Context, ignored: &[DocumentId]) -> Option<(&'c DocumentId, &'c Document)> {
    ctx.editor.documents
        .iter()
        .find(|(id, doc)| doc.is_modified() && !ignored.contains(id))
}

pub struct Dialog {
    modal: Modal,
    ignored_docs: Vec<DocumentId>,
}

impl Dialog {
    pub fn new() -> Self {
        let modal = Modal::new("⚠ Exit".into(), "".into());
        Self { modal, ignored_docs: vec![] }
    }

    fn ignore(ignored_docs: Vec<DocumentId>) -> Self {
        let modal = Modal::new("⚠ Exit".into(), "".into());
        Self { modal, ignored_docs }
    }

    fn yes(&mut self, ctx: &mut Context) -> EventResult {
        if let Some((id, _)) = doc(ctx, &self.ignored_docs) {
            let id = *id;
            ctx.editor.save_document(id);
        }
        let ignored = self.ignored_docs.clone();
        EventResult::Consumed(Some(Box::new(move |compositor: &mut Compositor, c: &mut Context| {
            compositor.pop();
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
            compositor.pop();
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

        self.modal.body = format!("Save changes to {}? ", doc.filename_display());
        self.modal.render_all(area, buffer);
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        if self.modal.confirm(event) {
            match self.modal.choice {
                Choice::Yes => return self.yes(ctx),
                Choice::No => return self.no(ctx),
                Choice::Cancel => return self.dismiss(),
            }
        }

        EventResult::Consumed(None)
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}
