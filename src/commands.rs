use log::debug;

use crate::{components::confirmation::Dialog, compositor::Component, editor::Editor};

pub struct Context<'a> {
    pub editor: &'a mut Editor,
    pub compositor_callbacks: Vec<crate::compositor::Callback>,
}

impl<'a> Context<'a> {
    fn push_component(&mut self, component: Box<dyn Component>) {
        self.compositor_callbacks.push(Box::new(|compositor, _| {
            compositor.push(component)
        }));
    }
}

pub struct Command {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub func: fn(&mut Context)
}

pub fn save(ctx: &mut Context) {
    ctx.editor.save_document();
}

pub fn quit(ctx: &mut Context) {
    if ctx.editor.document.modified {
        let text = format!(" Save changes to {}? ", ctx.editor.document.filename());
        let dialog = Dialog::new(
            " Exit".into(),
            text,
            Box::new(|ctx| {
                ctx.editor.save_document();
                ctx.editor.quit = true;
            }), Box::new(|ctx| {
                ctx.editor.quit = true;
            })
        );
        ctx.push_component(Box::new(dialog));
    } else {
        ctx.editor.quit = true;
    }
}

pub fn write_quit(ctx: &mut Context) {
    save(ctx);
    quit(ctx);
}

pub const COMMANDS: &[Command] = &[
    Command { name: "quit", aliases: &["q"], func: quit },
    Command { name: "write", aliases: &["save", "s", "write", "w"], func: save },
    Command { name: "write-quit", aliases: &["wq", "x"], func: write_quit },
];
