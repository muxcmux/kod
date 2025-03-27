use crate::compositor::Callback;
use crate::current;
use crate::ui::buffer::Buffer;
use crate::ui::modal::{FileOverwrite, FileReload, Modal, Okay, YesNoCancel};
use crate::{compositor::{Component, Context, EventResult}, ui::Rect};
use crossterm::event::KeyEvent;
use crate::document::{Document, DocumentId};

pub struct Alert {
    modal: Modal<Okay>
}

impl Alert {
    pub fn new(title: String, body: String) -> Self {
        let modal = Modal::new(title, body);
        Self { modal }
    }
}

impl Component for Alert {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        self.modal.render(area, buffer);
    }

    fn handle_key_event(&mut self, _event: KeyEvent, _ctx: &mut Context) -> EventResult {
        self.dismiss()
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}

fn next_unsaved_doc<'c>(ctx: &'c mut Context, ignored: &[DocumentId]) -> Option<(&'c DocumentId, &'c Document)> {
    ctx.editor.documents
        .iter()
        .find(|(id, doc)| doc.is_modified() && !ignored.contains(id))
}

pub struct EditorExit {
    modal: Modal,
    ignored_docs: Vec<DocumentId>,
}

impl EditorExit {
    pub fn new() -> Self {
        let modal = Modal::new("⚠ Exit".into(), "".into());
        Self { modal, ignored_docs: vec![] }
    }

    fn ignore(ignored_docs: Vec<DocumentId>) -> Self {
        let modal = Modal::new("⚠ Exit".into(), "".into());
        Self { modal, ignored_docs }
    }

    fn yes(&mut self, ctx: &mut Context) -> EventResult {
        if let Some((id, doc)) = next_unsaved_doc(ctx, &self.ignored_docs) {
            let id = *id;
            if doc.was_changed() {
                let ignored = self.ignored_docs.clone();
                return EventResult::Consumed(Some(Box::new(move |compositor, _| {
                    compositor.pop();
                    let mut confirm = OverwriteFile::new(id);
                    confirm.on_close = Some(Box::new(move |comp, cx| {
                        if next_unsaved_doc(cx, &ignored).is_some() {
                            comp.push(Box::new(EditorExit::ignore(ignored)))
                        } else {
                            cx.editor.quit();
                        }
                    }));
                    compositor.push(Box::new(confirm));
                })))
            } else {
                ctx.editor.save_document(id);
            }
        }

        let ignored = self.ignored_docs.clone();
        EventResult::Consumed(Some(Box::new(move |compositor, cx| {
            compositor.pop();
            if next_unsaved_doc(cx, &ignored).is_some() {
                compositor.push(Box::new(EditorExit::new()))
            } else {
                cx.editor.quit();
            }
        })))
    }

    fn no(&mut self, ctx: &mut Context) -> EventResult {
        let mut ignored = self.ignored_docs.clone();
        if let Some((id, _)) = next_unsaved_doc(ctx, &self.ignored_docs) {
            let id = *id;
            ignored.push(id);
        }

        EventResult::Consumed(Some(Box::new(move |compositor, cx| {
            compositor.pop();
            if next_unsaved_doc(cx, &ignored).is_some() {
                compositor.push(Box::new(EditorExit::ignore(ignored)));
            } else {
                cx.editor.quit();
            }
        })))
    }
}

impl Component for EditorExit {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let (_, doc) = next_unsaved_doc(ctx, &self.ignored_docs)
            .expect("Rendering the save confirmation dialog without unsaved docs shouldn't happen");

        self.modal.body = format!("Save changes to {}? ", doc.filename_display());
        self.modal.render(area, buffer);
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        if self.modal.handle_choice(event) {
            match self.modal.choice {
                YesNoCancel::Yes => return self.yes(ctx),
                YesNoCancel::No => return self.no(ctx),
                YesNoCancel::Cancel => return self.dismiss(),
            }
        }

        EventResult::Consumed(None)
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}

pub struct FileModified {
    modal: Modal<FileReload>,
}

impl FileModified {
    pub fn new() -> Self {
        let modal = Modal::new("⚠ File has changed".into(), "".into());
        Self { modal }
    }
}

impl Component for FileModified {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let (_, doc) = current!(ctx.editor);

        debug_assert!(doc.path.as_ref().is_some_and(|p| p.exists()));

        self.modal.body = format!("The file {:?} has been modified and the document has unsaved changes as well", doc.path.as_ref().unwrap());
        self.modal.render(area, buffer);
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        if self.modal.handle_choice(event) {
            match self.modal.choice {
                FileReload::Ok => return self.dismiss(),
                FileReload::Reload => {
                    let (_, doc) = current!(ctx.editor);
                    reload_file(doc.id, ctx);
                    return self.dismiss()
                },
            }
        }

        EventResult::Consumed(None)
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }
}


pub struct OverwriteFile {
    modal: Modal<FileOverwrite>,
    doc_id: DocumentId,
    on_close: Option<Callback>,
}

impl OverwriteFile {
    pub fn new(doc_id: DocumentId) -> Self {
        let modal = Modal::new("⚠ File changed".into(), "".into());
        Self { modal, doc_id, on_close: None }
    }
}

impl Component for OverwriteFile {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, ctx: &mut Context) {
        let doc = ctx.editor.documents.get_mut(&self.doc_id).unwrap();

        debug_assert!(doc.path.is_some());

        self.modal.body = format!("The file {:?} has been modified since last saved.", doc.path.as_ref().unwrap());
        self.modal.render(area, buffer);
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        true
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        if self.modal.handle_choice(event) {
            match self.modal.choice {
                FileOverwrite::Overwrite => {
                    ctx.editor.save_document(self.doc_id);
                    let on_close = std::mem::take(&mut self.on_close);

                    return EventResult::Consumed(Some(Box::new(move |compositor, cx| {
                        compositor.pop();
                        if let Some(on_close) = on_close {
                            on_close(compositor, cx)
                        }
                    })))
                },
                FileOverwrite::Reload => {
                    reload_file(self.doc_id, ctx);
                    let on_close = std::mem::take(&mut self.on_close);

                    return EventResult::Consumed(Some(Box::new(move |compositor, cx| {
                        compositor.pop();
                        if let Some(on_close) = on_close {
                            on_close(compositor, cx)
                        }
                    })))
                },
                FileOverwrite::Cancel => return self.dismiss(),
            }
        }

        EventResult::Consumed(None)
    }
}

fn reload_file(doc_id: DocumentId, ctx: &mut Context) {
    let panes = ctx.editor.doc_in_panes(doc_id);
    let doc = ctx.editor.documents.get_mut(&doc_id).unwrap();

    match doc.reload() {
        Ok(_) => {
            for pane_id in panes {
                let sel = doc.selection(pane_id);
                doc.set_selection(pane_id, sel.transform(|r| {
                    r.move_to(&doc.rope, None, None, &ctx.editor.mode)
                }));
            }
        }
        Err(e) => ctx.editor.set_error(e.to_string())
    }
}
