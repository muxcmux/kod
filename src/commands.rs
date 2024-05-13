use crate::compositor::Context;

pub struct Command {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub func: fn(&mut Context)
}

pub fn save(ctx: &mut Context) {
    ctx.editor.save_document();
}

pub fn quit(ctx: &mut Context) {
    ctx.editor.quit = true;
}

pub fn write_quit(ctx: &mut Context) {
    save(ctx);
    quit(ctx);
}

pub const COMMANDS: &[Command] = &[
    Command { name: "quit", aliases: &["q"], func: quit },
    Command { name: "quit", aliases: &["q"], func: quit },
    Command { name: "write", aliases: &["save", "s", "write", "w"], func: save },
    Command { name: "write-quit", aliases: &["wq", "x"], func: write_quit },
];
