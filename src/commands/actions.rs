use crossterm::event::KeyCode;
use smartstring::SmartString;

use crate::{editable_text::{EditableText, NEW_LINE}, editor::Mode, history::Transaction};

use super::{pallette::Pallette, Context};

fn enter_insert_mode_relative_to_cursor(x: usize, ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    for _ in 0..x {
        ctx.editor.document.text.cursor_right(&ctx.editor.mode);
    }
}

pub fn command_pallette(ctx: &mut Context) {
    let pallette = Box::new(Pallette::new());
    ctx.push_component(pallette);
}

pub fn enter_normal_mode(ctx: &mut Context) {
    ctx.editor.mode = Mode::Normal;
    ctx.editor.document.text.cursor_left(&ctx.editor.mode);
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

pub fn cursor_up(ctx: &mut Context) {
    ctx.editor.document.text.cursor_up(&ctx.editor.mode);
}

pub fn cursor_down(ctx: &mut Context) {
    ctx.editor.document.text.cursor_down(&ctx.editor.mode);
}

pub fn cursor_left(ctx: &mut Context) {
    ctx.editor.document.text.cursor_left(&ctx.editor.mode);
}

pub fn cursor_right(ctx: &mut Context) {
    ctx.editor.document.text.cursor_right(&ctx.editor.mode);
}

pub fn goto_first_line(ctx: &mut Context) {
    ctx.editor.document.text.move_cursor_to(None, Some(0), &ctx.editor.mode);
}

pub fn goto_last_line(ctx: &mut Context) {
    ctx.editor.document.text.move_cursor_to(None, Some(ctx.editor.document.text.rope.line_len() - 1), &ctx.editor.mode);
}

pub fn goto_line_first_non_whitespace(ctx: &mut Context) {
    ctx.editor.document.text.goto_line_first_non_whitespace(ctx.editor.document.text.cursor_y, &ctx.editor.mode);
}

pub fn goto_eol(ctx: &mut Context) {
    ctx.editor.document.text.move_cursor_to(Some(usize::MAX), None, &ctx.editor.mode);
}

pub fn goto_word_start_forward(ctx: &mut Context) {
    ctx.editor.document.text.goto_word_start_forward(&ctx.editor.mode);
}

pub fn goto_word_end_forward(ctx: &mut Context) {
    ctx.editor.document.text.goto_word_end_forward(&ctx.editor.mode);
}

pub fn goto_word_start_backward(ctx: &mut Context) {
    ctx.editor.document.text.goto_word_start_backward(&ctx.editor.mode);
}

pub fn goto_word_end_backward(ctx: &mut Context) {
    ctx.editor.document.text.goto_word_end_backward(&ctx.editor.mode);
}

pub fn goto_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.document.text.goto_character_forward(c, &ctx.editor.mode, 0);
        }
    })
}

pub fn goto_until_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.document.text.goto_character_forward(c, &ctx.editor.mode, 1);
        }
    })
}

pub fn goto_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.document.text.goto_character_backward(c, &ctx.editor.mode, 1);
        }
    })
}

pub fn goto_until_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            ctx.editor.document.text.goto_character_backward(c, &ctx.editor.mode, 0);
        }
    })
}

pub fn undo(ctx: &mut Context) {
    ctx.editor.document.undo_redo(true, &ctx.editor.mode);
}

pub fn redo(ctx: &mut Context) {
    ctx.editor.document.undo_redo(false, &ctx.editor.mode);
}

fn insert_char_at_offset(c: char, offset: usize, ctx: &mut Context) {
    let doc = &mut ctx.editor.document;

    let mut string = SmartString::new();
    string.push(c);

    doc.apply(
        &Transaction::change(
            &doc.text.rope,
            [(offset, offset, Some(string))].into_iter()
        ).set_cursor(&doc.text)
    );

    doc.modified = true;
}

pub fn append_character(c: char, ctx: &mut Context) {
    let offset = ctx.editor.document.text.byte_offset_at_cursor(
        ctx.editor.document.text.cursor_x,
        ctx.editor.document.text.cursor_y
    );
    insert_char_at_offset(c, offset, ctx);
    ctx.editor.document.text.cursor_right(&ctx.editor.mode);
}

pub fn append_new_line(ctx: &mut Context) {
    let offset = ctx.editor.document.text.byte_offset_at_cursor(
        ctx.editor.document.text.cursor_x,
        ctx.editor.document.text.cursor_y
    );
    insert_char_at_offset(NEW_LINE, offset, ctx);
    ctx.editor.document.text.cursor_down(&ctx.editor.mode);
}

pub fn insert_line_below(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    let offset = ctx.editor.document.text.rope.byte_of_line(ctx.editor.document.text.cursor_y) +
        ctx.editor.document.text.current_line_width();
    insert_char_at_offset(NEW_LINE, offset, ctx);
    ctx.editor.document.text.cursor_down(&ctx.editor.mode);
    ctx.editor.document.modified = true;
}

pub fn insert_line_above(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    let offset = ctx.editor.document.text.rope.byte_of_line(ctx.editor.document.text.cursor_y);
    insert_char_at_offset(NEW_LINE, offset, ctx);
    ctx.editor.document.text.move_cursor_to(Some(0), None, &ctx.editor.mode);
    ctx.editor.document.modified = true;
}

fn delete_to_the_left(text: &mut EditableText , mode: &Mode) -> Option<(usize, usize)> {
    if text.cursor_x > 0 {
        let mut start = text.rope.byte_of_line(text.cursor_y);
        let mut end = start;
        let idx = text.grapheme_at_cursor().0 - 1;

        for (i, g) in text.current_line().graphemes().enumerate() {
            if i < idx { start += g.len() }
            if i == idx {
                end = start + g.len();
                break
            }
        }

        text.cursor_left(mode);

        return Some((start, end));
    } else if text.cursor_y > 0  {
        let to = text.rope.byte_of_line(text.cursor_y);
        let from = to.saturating_sub(NEW_LINE.len_utf8());

        text.move_cursor_to(Some(text.line_width(text.cursor_y - 1)), Some(text.cursor_y - 1), mode);

        return Some((from, to));
    }

    None
}

pub fn delete_symbol_to_the_left(ctx: &mut Context) {
    if let Some((from, to)) = delete_to_the_left(&mut ctx.editor.document.text, &ctx.editor.mode) {
        ctx.editor.document.apply(
            &Transaction::change(
                &ctx.editor.document.text.rope,
                [(from, to, None)].into_iter()
            ).set_cursor(&ctx.editor.document.text)
        );
        ctx.editor.document.modified = true;
    }
}

fn delete_lines(from: usize, size: usize, ctx: &mut Context) -> bool {
    let text = &mut ctx.editor.document.text;

    if text.is_blank() { return false }

    let to = (from + size).min(text.rope.line_len());

    let s = text.rope.line_slice(from..to);

    let start = text.rope.byte_of_line(from);
    let mut end = start + s.byte_len();

    // if we are deleting everything, remember to leave the newline byte
    if start == 0 && to == text.rope.line_len() {
        end -= NEW_LINE.len_utf8();
    }

    let t = Transaction::change(&text.rope,
        [(start, end, None)].into_iter()
    ).set_cursor(text);

    ctx.editor.document.apply(&t);

    true
}

pub fn delete_current_line(ctx: &mut Context) {
    if delete_lines(ctx.editor.document.text.cursor_y, 1, ctx) {
        if ctx.editor.document.text.cursor_y > ctx.editor.document.text.rope.line_len().saturating_sub(1) {
            ctx.editor.document.text.cursor_up(&ctx.editor.mode);
        } else {
            ctx.editor.document.text.move_cursor_to(None, None, &ctx.editor.mode);
        }
        ctx.editor.document.modified = true;
    }
}

pub fn delete_until_eol(ctx: &mut Context) {
    if let Some((start, end)) = ctx.editor.document.text.byte_range_until_eol() {
        ctx.editor.document.apply(&Transaction::change(&ctx.editor.document.text.rope,
            [(start, end, None)].into_iter()
            ).set_cursor(&ctx.editor.document.text)
        );
        ctx.editor.document.text.move_cursor_to(None, None, &ctx.editor.mode);
        ctx.editor.document.modified = true;
    }
}

pub fn change_until_eol(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    delete_until_eol(ctx);
}
