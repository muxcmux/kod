pub mod actions;
pub mod palette;

use crossterm::event::KeyEvent;

use crate::{components::save_documents::Dialog, compositor::Component, current, doc, editor::Editor, panes::Layout};

pub type KeyCallback = Box<dyn FnOnce(&mut Context, KeyEvent)>;

pub struct Context<'a> {
    pub editor: &'a mut Editor,
    pub compositor_callbacks: Vec<crate::compositor::Callback>,
    pub on_next_key_callback: Option<KeyCallback>,
}

impl Context<'_> {
    fn push_component(&mut self, component: Box<dyn Component>) {
        self.compositor_callbacks.push(Box::new(|compositor, _| {
            compositor.push(component)
        }));
    }

    fn on_next_key(&mut self, fun: impl FnOnce(&mut Context, KeyEvent) + 'static) {
        self.on_next_key_callback = Some(Box::new(fun));
    }
}

pub struct Command {
    pub name: &'static str,
    pub desc: &'static str,
    pub aliases: &'static [&'static str],
    pub func: fn(&mut Context)
}

pub fn save(ctx: &mut Context) {
    let doc = doc!(ctx.editor);
    let id = doc.id;
    ctx.editor.save_document(id);
}

pub fn quit(ctx: &mut Context) {
    if ctx.editor.panes.panes.len() == 1 {
        if ctx.editor.has_unsaved_docs() {
            ctx.push_component(Box::new(Dialog::new()));
        } else {
            ctx.editor.quit();
        }
    } else {
        ctx.editor.panes.close(ctx.editor.panes.focus);
    }
}

pub fn write_quit(ctx: &mut Context) {
    save(ctx);
    quit(ctx);
}

pub fn split_horizontally(ctx: &mut Context) {
    let (_, doc) = current!(ctx.editor);
    ctx.editor.panes.split(Layout::Vertical, doc);
}

pub fn split_vertically(ctx: &mut Context) {
    let (_, doc) = current!(ctx.editor);
    ctx.editor.panes.split(Layout::Horizontal, doc);
}

pub fn toggle_readonly(ctx: &mut Context) {
    let (_, doc) = current!(ctx.editor);
    doc.readonly = !doc.readonly;
    let ro = if doc.readonly { "ON" } else { "OFF" };
    ctx.editor.set_status(format!("Readonly {ro}"));
}

pub const COMMANDS: &[Command] = &[
    Command { name: "write", aliases: &["write", "w"], desc: "Save file to disc", func: save },
    Command { name: "quit", aliases: &["q", "Q", "exit"], desc: "Exit kod", func: quit },
    Command { name: "write-quit", aliases: &["wq", "x"], desc: "Save file to disc and exit", func: write_quit },
    Command { name: "split", aliases: &["s"], desc: "Split pane horizontally", func: split_horizontally },
    Command { name: "vsplit", aliases: &["vs"], desc: "Split pane vertically", func: split_vertically },
    Command { name: "readonly", aliases: &["ro"], desc: "Toggle document readonly mode", func: toggle_readonly },
];
