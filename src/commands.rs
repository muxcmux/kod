pub mod actions;
pub mod pallette;

use crossterm::event::KeyEvent;

use crate::{components::confirmation::Dialog, compositor::Component, doc, editor::Editor, panes::Layout, ui::borders::BorderType};

pub type KeyCallback = Box<dyn FnOnce(&mut Context, KeyEvent)>;

pub struct Context<'a> {
    pub editor: &'a mut Editor,
    pub compositor_callbacks: Vec<crate::compositor::Callback>,
    pub on_next_key_callback: Option<KeyCallback>,
}

impl<'a> Context<'a> {
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
    ctx.editor.save_document();
}

pub fn quit(ctx: &mut Context) {
    let doc = doc!(ctx.editor);
    if doc.modified {
        let text = format!(" Save changes to {}? ", doc.filename());
        let dialog = Dialog::new(
            "Exit".into(),
            text,
            BorderType::Rounded,
            Box::new(|ctx| {
                ctx.editor.save_document();
                ctx.editor.quit = true;
            }), Box::new(|ctx| {
                ctx.editor.quit = true;
            })
        );
        ctx.push_component(Box::new(dialog));
    } else if ctx.editor.panes.panes.len() == 1 {
        ctx.editor.quit = true;
    } else {
        ctx.editor.quit = true;
        //ctx.editor.panes.close(ctx.editor.panes.focused_id);
    }
}

pub fn write_quit(ctx: &mut Context) {
    save(ctx);
    quit(ctx);
}

pub fn horizontal_split(ctx: &mut Context) {
    ctx.editor.panes.split(Layout::Horizontal);
}

pub fn vertical_split(ctx: &mut Context) {
    ctx.editor.panes.split(Layout::Vertical);
}

pub const COMMANDS: &[Command] = &[
    Command { name: "write", aliases: &["save", "s", "write", "w"], desc: "Save file to disc", func: save },
    Command { name: "quit", aliases: &["q", "Q", "exit"], desc: "Exit kod", func: quit },
    Command { name: "write-quit", aliases: &["wq", "x"], desc: "Save file to disc and exit", func: write_quit },
    Command { name: "split", aliases: &[], desc: "Split pane horizontally", func: horizontal_split },
    Command { name: "vsplit", aliases: &["vs"], desc: "Split pane vertically", func: vertical_split },
];
