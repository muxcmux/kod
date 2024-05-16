use crate::{document::NEW_LINE, editor::{Editor, Mode}};

pub struct Context<'a> {
    pub editor: &'a mut Editor,
    pub compositor_callbacks: Vec<crate::compositor::Callback>,
}

fn enter_insert_mode_relative_to_cursor(x: usize, ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    for _ in 0..x {
        ctx.editor.document.cursor_right(&ctx.editor.mode);
    }
}

pub fn enter_normal_mode(ctx: &mut Context) {
    ctx.editor.mode = Mode::Normal;
    ctx.editor.document.cursor_left(&ctx.editor.mode);
}

pub fn enter_insert_mode_at_cursor(ctx: &mut Context) {
    enter_insert_mode_relative_to_cursor(0, ctx);
}

pub fn enter_insert_mode_at_first_non_whitespace(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    goto_line_first_non_whitespace(ctx);
}

pub fn enter_insert_mode_after_cursor(ctx: &mut Context) {
    enter_insert_mode_relative_to_cursor(1, ctx);
}

pub fn enter_insert_mode_at_eol(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    goto_eol(ctx);
}

pub fn append_character(c: char, ctx: &mut Context) {
    ctx.editor.document.insert_char_at_cursor(c, &ctx.editor.mode);
}

pub fn append_new_line(ctx: &mut Context) {
    ctx.editor.document.insert_char_at_cursor(NEW_LINE, &ctx.editor.mode);
}

pub fn cursor_up(ctx: &mut Context) {
    ctx.editor.document.cursor_up(&ctx.editor.mode);
}

pub fn cursor_down(ctx: &mut Context) {
    ctx.editor.document.cursor_down(&ctx.editor.mode);
}

pub fn cursor_left(ctx: &mut Context) {
    ctx.editor.document.cursor_left(&ctx.editor.mode);
}

pub fn cursor_right(ctx: &mut Context) {
    ctx.editor.document.cursor_right(&ctx.editor.mode);
}

pub fn goto_first_line(ctx: &mut Context) {
    ctx.editor.document.move_cursor_to(None, Some(0), &ctx.editor.mode);
}

pub fn goto_last_line(ctx: &mut Context) {
    ctx.editor.document.move_cursor_to(None, Some(ctx.editor.document.lines_len() - 1), &ctx.editor.mode);
}

pub fn goto_line_first_non_whitespace(ctx: &mut Context) {
    for (i, c) in ctx.editor.document.current_line().chars().enumerate() {
        if !c.is_whitespace() {
            ctx.editor.document.move_cursor_to(Some(i), None, &ctx.editor.mode);
            break;
        }
    }
}

pub fn goto_eol(ctx: &mut Context) {
    ctx.editor.document.move_cursor_to(Some(ctx.editor.document.current_line_len()), None, &ctx.editor.mode);
}

pub fn insert_line_below(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    ctx.editor.document.move_cursor_to(Some(std::usize::MAX), None, &ctx.editor.mode);
    ctx.editor.document.insert_char_at_cursor('\n', &ctx.editor.mode);
}

pub fn insert_line_above(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    ctx.editor.document.move_cursor_to(Some(std::usize::MAX), Some(ctx.editor.document.cursor_y.saturating_sub(1)), &ctx.editor.mode);
    ctx.editor.document.insert_char_at_cursor('\n', &ctx.editor.mode);
}

pub fn delete_symbol_to_the_left(ctx: &mut Context) {
    ctx.editor.document.delete_to_the_left(&ctx.editor.mode);
}

pub fn delete_current_line(ctx: &mut Context) {
    ctx.editor.document.delete_lines(ctx.editor.document.cursor_y, ctx.editor.document.cursor_y, &ctx.editor.mode);
}

pub fn delete_until_eol(ctx: &mut Context) {
    ctx.editor.document.delete_until_eol(&ctx.editor.mode);
}

pub fn change_until_eol(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    ctx.editor.document.delete_until_eol(&ctx.editor.mode);
}
