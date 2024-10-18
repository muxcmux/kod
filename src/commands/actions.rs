use crop::Rope;
use crossterm::event::KeyCode;
use smartstring::SmartString;

use crate::{document::Document, editor::Mode, graphemes::NEW_LINE, history::Transaction, panes::{Direction, Pane}};

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
        let pane = $editor.panes.panes.get(&$editor.panes.focus).expect("Can't get focused pane");
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
        $editor.panes.panes.get_mut($id).expect(format!("Couldn't get pane with id: {:?}", $id))
    }};
    ($editor:expr) => {{
        $editor.panes.panes.get_mut(&$editor.panes.focus).expect("Couldn't get focused pane")
    }};
}

/// Get the current pane immutably
/// Returns `&Pane`
#[macro_export]
macro_rules! pane {
    ($editor:expr, $id:expr) => {{
        $editor.panes.panes.get($id).expect(format!("Couldn't get pane with id: {:?}", $id))
    }};
    ($editor:expr) => {{
        $editor.panes.panes.get(&$editor.panes.focus).expect("Couldn't get focused pane")
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
    let (pane, doc) = current!(ctx.editor);
    for _ in 0..x {
        pane.view.cursor_right(&doc.rope, &ctx.editor.mode);
    }
}

pub fn command_pallette(ctx: &mut Context) {
    let pallette = Box::new(Pallette::new());
    ctx.push_component(pallette);
}

pub fn enter_normal_mode(ctx: &mut Context) {
    ctx.editor.mode = Mode::Normal;
    let (pane, doc) = current!(ctx.editor);
    pane.view.cursor_left(&doc.rope, &ctx.editor.mode);
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
    let (pane, doc) = current!(ctx.editor);
    pane.view.cursor_up(&doc.rope, &ctx.editor.mode);
}

pub fn cursor_down(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.cursor_down(&doc.rope, &ctx.editor.mode);
}

pub fn cursor_left(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.cursor_left(&doc.rope, &ctx.editor.mode);
}

pub fn cursor_right(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.cursor_right(&doc.rope, &ctx.editor.mode);
}

pub fn half_page_up(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let y = pane.view.text_cursor_y.saturating_sub(half);
    pane.view.move_cursor_to(&doc.rope, None, Some(y), &ctx.editor.mode);
}

pub fn half_page_down(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let half = (pane.area.height / 2) as usize;
    let y = pane.view.text_cursor_y + half;
    pane.view.move_cursor_to(&doc.rope, None, Some(y), &ctx.editor.mode);
}

pub fn goto_first_line(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.move_cursor_to(&doc.rope, None, Some(0), &ctx.editor.mode);
}

pub fn goto_last_line(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.move_cursor_to(&doc.rope, None, Some(doc.rope.line_len() - 1), &ctx.editor.mode);
}

pub fn goto_line_first_non_whitespace(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.goto_line_first_non_whitespace(&doc.rope, pane.view.text_cursor_y, &ctx.editor.mode);
}

pub fn goto_eol(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.move_cursor_to(&doc.rope, Some(usize::MAX), None, &ctx.editor.mode);
}

pub fn goto_word_start_forward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.goto_word_start_forward(&doc.rope, &ctx.editor.mode);
}

pub fn goto_word_end_forward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.goto_word_end_forward(&doc.rope, &ctx.editor.mode);
}

pub fn goto_word_start_backward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.goto_word_start_backward(&doc.rope, &ctx.editor.mode);
}

pub fn goto_word_end_backward(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    pane.view.goto_word_end_backward(&doc.rope, &ctx.editor.mode);
}

pub fn goto_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            let (pane, doc) = current!(ctx.editor);
            pane.view.goto_character_forward(&doc.rope, c, &ctx.editor.mode, 0);
        }
    })
}

pub fn goto_until_character_forward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            let (pane, doc) = current!(ctx.editor);
            pane.view.goto_character_forward(&doc.rope, c, &ctx.editor.mode, 1);
        }
    })
}

pub fn goto_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            let (pane, doc) = current!(ctx.editor);
            pane.view.goto_character_backward(&doc.rope, c, &ctx.editor.mode, 1);
        }
    })
}

pub fn goto_until_character_backward(ctx: &mut Context) {
    ctx.on_next_key(|ctx, event| {
        if let KeyCode::Char(c) = event.code {
            let (pane, doc) = current!(ctx.editor);
            pane.view.goto_character_backward(&doc.rope, c, &ctx.editor.mode, 0);
        }
    })
}

pub fn undo(ctx: &mut Context) {
    current!(ctx.editor).1.undo_redo(true, &ctx.editor.mode);
}

pub fn redo(ctx: &mut Context) {
    current!(ctx.editor).1.undo_redo(false, &ctx.editor.mode);
}

fn insert_or_replace_char_at_offset(c: char, offset_start: usize, offset_end: usize, pane: &Pane, doc: &mut Document) {
    let mut string = SmartString::new();
    string.push(c);

    doc.apply(
        &Transaction::change(
            &doc.rope,
            [(offset_start, offset_end, Some(string))].into_iter()
        ).set_cursor(pane.view.text_cursor_x, pane.view.text_cursor_y)
    );

    doc.modified = true;
}

pub fn append_character(c: char, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let offset = pane.view.byte_offset_at_cursor(
        &doc.rope,
        pane.view.text_cursor_x,
        pane.view.text_cursor_y
    );
    insert_or_replace_char_at_offset(c, offset, offset, pane, doc);
    pane.view.cursor_right(&doc.rope, &ctx.editor.mode);
}

pub fn append_or_replace_character(c: char, ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let mut start_byte = doc.rope.byte_of_line(pane.view.text_cursor_y);
    let mut end_byte = start_byte;

    let mut col = 0;

    for g in doc.rope.line(pane.view.text_cursor_y).graphemes() {
        let width = unicode_display_width::width(&g) as usize;
        let size = g.bytes().count();

        if col >= pane.view.text_cursor_x {
            end_byte = start_byte + size;
            break;
        }

        col += width;
        start_byte += size;
    }

    insert_or_replace_char_at_offset(c, start_byte, end_byte.max(start_byte), pane, doc);
    pane.view.cursor_right(&doc.rope, &ctx.editor.mode);
}

pub fn append_new_line(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    let offset = pane.view.byte_offset_at_cursor(
        &doc.rope,
        pane.view.text_cursor_x,
        pane.view.text_cursor_y
    );
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, pane, doc);
    pane.view.cursor_down(&doc.rope, &ctx.editor.mode);
}

pub fn insert_line_below(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    let (pane, doc) = current!(ctx.editor);
    let offset = doc.rope.byte_of_line(pane.view.text_cursor_y) +
        doc.rope.line(pane.view.text_cursor_y).byte_len();
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, pane, doc);
    pane.view.cursor_down(&doc.rope, &ctx.editor.mode);
}

pub fn insert_line_above(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    let (pane, doc) = current!(ctx.editor);
    let offset = doc.rope.byte_of_line(pane.view.text_cursor_y);
    insert_or_replace_char_at_offset(NEW_LINE, offset, offset, pane, doc);
    pane.view.move_cursor_to(&doc.rope, Some(0), None, &ctx.editor.mode);
}

fn delete_to_the_left(pane: &mut Pane, rope: &Rope , mode: &Mode) -> Option<(usize, usize)> {
    if pane.view.text_cursor_x > 0 {
        let mut start = rope.byte_of_line(pane.view.text_cursor_y);
        let mut end = start;
        let idx = pane.view.grapheme_at_cursor(rope).0 - 1;

        for (i, g) in rope.line(pane.view.text_cursor_y).graphemes().enumerate() {
            if i < idx { start += g.len() }
            if i == idx {
                end = start + g.len();
                break
            }
        }

        pane.view.cursor_left(rope, mode);

        return Some((start, end));
    } else if pane.view.text_cursor_y > 0  {
        let to = rope.byte_of_line(pane.view.text_cursor_y);
        let from = to.saturating_sub(NEW_LINE.len_utf8());

        pane.view.move_cursor_to(rope, Some(pane.view.line_width(rope, pane.view.text_cursor_y - 1)), Some(pane.view.text_cursor_y - 1), mode);

        return Some((from, to));
    }

    None
}

pub fn delete_symbol_to_the_left(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    if let Some((from, to)) = delete_to_the_left(pane, &mut doc.rope, &ctx.editor.mode) {
        doc.apply(
            &Transaction::change(
                &doc.rope,
                [(from, to, None)].into_iter()
            ).set_cursor(pane.view.text_cursor_x, pane.view.text_cursor_y)
        );
        doc.modified = true;
    }
}

fn delete_lines(from: usize, size: usize, pane: &Pane, doc: &mut Document) -> bool {
    let rope = &mut doc.rope;

    if pane.view.is_blank(rope) { return false }

    let to = (from + size).min(rope.line_len());

    let s = rope.line_slice(from..to);

    let start = rope.byte_of_line(from);
    let mut end = start + s.byte_len();

    // if we are deleting everything, remember to leave the newline byte
    if start == 0 && to == rope.line_len() {
        end -= NEW_LINE.len_utf8();
    }

    let t = Transaction::change(&rope,
        [(start, end, None)].into_iter()
    ).set_cursor(pane.view.text_cursor_x, pane.view.text_cursor_y);

    doc.apply(&t);

    true
}

pub fn delete_current_line(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    if delete_lines(pane.view.text_cursor_y, 1, pane, doc) {
        if pane.view.text_cursor_y > doc.rope.line_len().saturating_sub(1) {
            pane.view.cursor_up(&doc.rope, &ctx.editor.mode);
        } else {
            pane.view.move_cursor_to(&doc.rope, None, None, &ctx.editor.mode);
        }
        doc.modified = true;
    }
}

pub fn delete_until_eol(ctx: &mut Context) {
    let (pane, doc) = current!(ctx.editor);
    if let Some((start, end)) = pane.view.byte_range_until_eol(&doc.rope) {
        doc.apply(&Transaction::change(&doc.rope,
            [(start, end, None)].into_iter()
            ).set_cursor(pane.view.text_cursor_x, pane.view.text_cursor_y)
        );
        pane.view.move_cursor_to(&doc.rope, None, None, &ctx.editor.mode);
        doc.modified = true;
    }
}

pub fn change_until_eol(ctx: &mut Context) {
    ctx.editor.mode = Mode::Insert;
    delete_until_eol(ctx);
}

pub fn switch_pane_top(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Up);
}

pub fn switch_pane_bottom(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Down);
}

pub fn switch_pane_left(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Left);
}

pub fn switch_pane_right(ctx: &mut Context) {
    ctx.editor.panes.switch(Direction::Right);
}
