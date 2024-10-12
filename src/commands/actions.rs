use crossterm::event::KeyCode;
use smartstring::SmartString;

use crate::{document::Document, editable_text::{EditableText, NEW_LINE}, editor::Mode, history::Transaction};

use super::{pallette::Pallette, Context};

// From helix:
// These are macros to make getting very nested fields in the `Editor` struct easier
// These are macros instead of functions because functions will have to take `&mut self`
// However, rust doesn't know that you only want a partial borrow instead of borrowing the
// entire struct which `&mut self` says.  This makes it impossible to do other mutable
// stuff to the struct because it is already borrowed. Because macros are expanded,
// this circumvents the problem because it is just like indexing fields by hand and then
// putting a `&mut` in front of it. This way rust can see that we are only borrowing a
// part of the struct and not the entire thing.

/// Get the current pane and document mutably as a tuple.
/// Returns `(&mut Pane, &mut Document)`
#[macro_export]
macro_rules! current {
    ($editor:expr) => {{
        let pane = $crate::pane_mut!($editor);
        let doc = $crate::doc_mut!($editor, &pane.doc_id);
        (pane, doc)
    }};
}

/// Get the current pane and document as immutable refs
#[macro_export]
macro_rules! current_ref {
    ($editor:expr) => {{
        let pane = $editor.panes.panes.get(&$editor.panes.focused_id).expect("Can't get focused pane");
        let doc = &$editor.documents[&pane.doc_id];
        (pane, doc)
    }};
}

/// Get the current document mutably.
/// Returns `&mut Document`
#[macro_export]
macro_rules! doc_mut {
    ($editor:expr, $id:expr) => {{
        $editor.documents.get_mut($id).unwrap()
    }};
    ($editor:expr) => {{
        $crate::current!(&$editor).1
    }};
}

/// Get the current pane mutably.
/// Returns `&mut Pane`
#[macro_export]
macro_rules! pane_mut {
    ($editor:expr, $id:expr) => {{
        $editor.panes.panes.get_mut($id).expect("Couldn't get pane with id: {}", $id)
    }};
    ($editor:expr) => {{
        $editor.panes.panes.get_mut(&$editor.panes.focused_id).expect("Couldn't get focused pane")
    }};
}

/// Get the current pane immutably
/// Returns `&Pane`
#[macro_export]
macro_rules! pane {
    ($editor:expr, $id:expr) => {{
        $editor.panes.panes.get($id).expect("Couldn't get pane with id: {}", $id)
    }};
    ($editor:expr) => {{
        $editor.panes.panes.get(&$editor.panes.focused_id).expect("Couldn't get focused pane")
    }};
}

/// Get an immutable reference to the current doc
#[macro_export]
macro_rules! doc {
    ($editor:expr, $id:expr) => {{
        &$editor.documents[$id]
    }};
    ($editor:expr) => {{
        $crate::current_ref!($editor).1
    }};
}

fn enter_insert_mode_relative_to_cursor(x: usize, ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    let doc = current!(ctx.editor).1;
    for _ in 0..x {
        doc.text.cursor_right(&ctx.editor.mode);
    }
}

pub fn command_pallette(ctx: &mut Context) {
    let pallette = Box::new(Pallette::new());
    ctx.push_component(pallette);
}

pub fn enter_normal_mode(ctx: &mut Context) {
    ctx.editor.mode = Mode::Normal;
    current!(ctx.editor).1.text.cursor_left(&ctx.editor.mode);
}

pub fn enter_replace_mode(ctx: &mut Context) {
    ctx.editor.mode = Mode::Replace;
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
    current!(ctx.editor).1.text.cursor_up(&ctx.editor.mode);
}

pub fn cursor_down(ctx: &mut Context) {
    current!(ctx.editor).1.text.cursor_down(&ctx.editor.mode);
}

pub fn cursor_left(ctx: &mut Context) {
    current!(ctx.editor).1.text.cursor_left(&ctx.editor.mode);
}

pub fn cursor_right(ctx: &mut Context) {
    current!(ctx.editor).1.text.cursor_right(&ctx.editor.mode);
}

pub fn half_page_up(ctx: &mut Context) {
    let (view, doc) = current!(ctx.editor);
    let half = (view.area.height / 2) as usize;
    let y = doc.text.cursor_y.saturating_sub(half);
    doc.text.move_cursor_to(None, Some(y), &ctx.editor.mode);
}

pub fn half_page_down(ctx: &mut Context) {
    let (view, doc) = current!(ctx.editor);
    let half = (view.area.height / 2) as usize;
    let y = doc.text.cursor_y + half;
    doc.text.move_cursor_to(None, Some(y), &ctx.editor.mode);
}

pub fn goto_first_line(ctx: &mut Context) {
    current!(ctx.editor).1.text.move_cursor_to(None, Some(0), &ctx.editor.mode);
}

pub fn goto_last_line(ctx: &mut Context) {
    let doc = current!(ctx.editor).1;
    doc.text.move_cursor_to(None, Some(doc.text.rope.line_len() - 1), &ctx.editor.mode);
}

pub fn goto_line_first_non_whitespace(ctx: &mut Context) {
    let doc = current!(ctx.editor).1;
    doc.text.goto_line_first_non_whitespace(doc.text.cursor_y, &ctx.editor.mode);
}

pub fn goto_eol(ctx: &mut Context) {
    current!(ctx.editor).1.text.move_cursor_to(Some(usize::MAX), None, &ctx.editor.mode);
}

pub fn goto_word_start_forward(ctx: &mut Context) {
    current!(ctx.editor).1.text.goto_word_start_forward(&ctx.editor.mode);
}

pub fn goto_word_end_forward(ctx: &mut Context) {
    current!(ctx.editor).1.text.goto_word_end_forward(&ctx.editor.mode);
}

pub fn goto_word_start_backward(ctx: &mut Context) {
    current!(ctx.editor).1.text.goto_word_start_backward(&ctx.editor.mode);
}

pub fn goto_word_end_backward(ctx: &mut Context) {
    current!(ctx.editor).1.text.goto_word_end_backward(&ctx.editor.mode);
}

pub fn goto_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            current!(ctx.editor).1.text.goto_character_forward(c, &ctx.editor.mode, 0);
        }
    })
}

pub fn goto_until_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            current!(ctx.editor).1.text.goto_character_forward(c, &ctx.editor.mode, 1);
        }
    })
}

pub fn goto_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            current!(ctx.editor).1.text.goto_character_backward(c, &ctx.editor.mode, 1);
        }
    })
}

pub fn goto_until_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            current!(ctx.editor).1.text.goto_character_backward(c, &ctx.editor.mode, 0);
        }
    })
}

pub fn undo(ctx: &mut Context) {
    current!(ctx.editor).1.undo_redo(true, &ctx.editor.mode);
}

pub fn redo(ctx: &mut Context) {
    current!(ctx.editor).1.undo_redo(false, &ctx.editor.mode);
}

fn insert_or_replace_char_at_offset(c: char, offset_start: usize, offset_end: usize, doc: &mut Document) {
    let mut string = SmartString::new();
    string.push(c);

    doc.apply(
        &Transaction::change(
            &doc.text.rope,
            [(offset_start, offset_end, Some(string))].into_iter()
        ).set_cursor(&doc.text)
    );

    doc.modified = true;
}

pub fn append_character(c: char, ctx: &mut Context) {
    let doc = current!(ctx.editor).1;
    let offset = doc.text.byte_offset_at_cursor(
        doc.text.cursor_x,
        doc.text.cursor_y
    );
    insert_or_replace_char_at_offset(c, offset, offset, doc);
    doc.text.cursor_right(&ctx.editor.mode);
}

pub fn append_or_replace_character(c: char, ctx: &mut Context) {
    let doc = current!(ctx.editor).1;
    let mut start_byte = doc.text.rope.byte_of_line(doc.text.cursor_y);
    let mut end_byte = start_byte;

    let mut col = 0;

    for g in doc.text.current_line().graphemes() {
        let width = unicode_display_width::width(&g) as usize;
        let size = g.bytes().count();

        if col >= doc.text.cursor_x {
            end_byte = start_byte + size;
            break;
        }

        col += width;
        start_byte += size;
    }

    insert_or_replace_char_at_offset(c, start_byte, end_byte.max(start_byte), doc);
    doc.text.cursor_right(&ctx.editor.mode);
}

pub fn append_new_line(ctx: &mut Context) {
    let doc = current!(ctx.editor).1;
    let offset = doc.text.byte_offset_at_cursor(
        doc.text.cursor_x,
        doc.text.cursor_y
    );
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, doc);
    doc.text.cursor_down(&ctx.editor.mode);
}

pub fn insert_line_below(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    let doc = current!(ctx.editor).1;
    let offset = doc.text.rope.byte_of_line(doc.text.cursor_y) +
        doc.text.rope.line(doc.text.cursor_y).byte_len();
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, doc);
    doc.text.cursor_down(&ctx.editor.mode);
    doc.modified = true;
}

pub fn insert_line_above(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    let doc = current!(ctx.editor).1;
    let offset = doc.text.rope.byte_of_line(doc.text.cursor_y);
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, doc);
    doc.text.move_cursor_to(Some(0), None, &ctx.editor.mode);
    doc.modified = true;
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
    let doc = current!(ctx.editor).1;
    if let Some((from, to)) = delete_to_the_left(&mut doc.text, &ctx.editor.mode) {
        doc.apply(
            &Transaction::change(
                &doc.text.rope,
                [(from, to, None)].into_iter()
            ).set_cursor(&doc.text)
        );
        doc.modified = true;
    }
}

fn delete_lines(from: usize, size: usize, doc: &mut Document) -> bool {
    let text = &mut doc.text;

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

    doc.apply(&t);

    true
}

pub fn delete_current_line(ctx: &mut Context) {
    let doc = current!(ctx.editor).1;
    if delete_lines(doc.text.cursor_y, 1, doc) {
        if doc.text.cursor_y > doc.text.rope.line_len().saturating_sub(1) {
            doc.text.cursor_up(&ctx.editor.mode);
        } else {
            doc.text.move_cursor_to(None, None, &ctx.editor.mode);
        }
        doc.modified = true;
    }
}

pub fn delete_until_eol(ctx: &mut Context) {
    let doc = current!(ctx.editor).1;
    if let Some((start, end)) = doc.text.byte_range_until_eol() {
        doc.apply(&Transaction::change(&doc.text.rope,
            [(start, end, None)].into_iter()
            ).set_cursor(&doc.text)
        );
        doc.text.move_cursor_to(None, None, &ctx.editor.mode);
        doc.modified = true;
    }
}

pub fn change_until_eol(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    delete_until_eol(ctx);
}
