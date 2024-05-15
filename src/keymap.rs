use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent};
use crate::{actions::*, editor::Mode};

type Func = fn(&mut Context);
type Keymap = HashMap<KeyEvent, Action>;

#[derive(Debug)]
pub struct Keymaps {
    map: HashMap<Mode, Keymap>,
    pending: Vec<KeyEvent>,
}

impl Keymaps {
    pub fn default() -> Self {
        let mut map = HashMap::new();
        map.insert(Mode::Normal, normal_mode_keymap());
        map.insert(Mode::Insert, insert_mode_keymap());

        Self { map, pending: vec![] }
    }

    pub fn get(&mut self, mode: &Mode, event: KeyEvent) -> KeymapResult {
        // gets the keymap for the mode
        let keymap = self.map.get(mode).unwrap_or_else(|| panic!("No keymap found for editor mode {:?}", mode));

        // esc key clears the pending keys and returns a cancelled
        // event with the current pending keys, so they can be
        // used elsewhere
        if matches!(event.code, KeyCode::Esc) && !self.pending.is_empty() {
            return KeymapResult::Cancelled(self.pending.drain(..).collect());
        }

        // get the action for the root key in the keymap
        let root = self.pending.first().unwrap_or(&event);

        // if the action is a function, or the key isn't mapped,
        // short circuit and return a result with the function or not found
        let action = match keymap.get(root) {
            None => { return KeymapResult::NotFound },
            Some(Action::Func(f)) => { return KeymapResult::Found(*f) }
            Some(keymap) => keymap,
        };

        // otherwise push the current key code to the pending keys
        self.pending.push(event);

        // and search for an action in this action's keymap
        match action.find_by_path(&self.pending[1..]) {
            None => KeymapResult::Cancelled(self.pending.drain(..).collect()),
            Some(Action::Map(_)) => KeymapResult::Pending,
            Some(Action::Func(f)) => {
                self.pending.clear();
                KeymapResult::Found(*f)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Func(Func),
    Map(Keymap)
}

impl Action {
    pub fn find_by_path(&self, path: &[KeyEvent]) -> Option<&Self> {
        let mut current = self;

        for key in path {
            current = match current {
                Action::Map(map) => map.get(key),
                Action::Func(_) => None,
            }?
        }

        Some(current)
    }
}

pub enum KeymapResult {
    Found(Func),
    Pending,
    Cancelled(Vec<KeyEvent>),
    NotFound,
}

fn g_keymap() -> Keymap {
    let mut map = Keymap::new();
    map.insert(KeyCode::Char('g').into(), Action::Func(goto_first_line));

    map
}

fn d_keymap() -> Keymap {
    use KeyCode::*;
    use Action::*;

    Keymap::from([
        (Char('d').into(), Func(delete_current_line))
    ])
}

fn normal_mode_keymap() -> Keymap {
    use KeyCode::*;
    use Action::*;

    Keymap::from([
        (Char('h').into(), Func(cursor_left)),
        (Left.into(),      Func(cursor_left)),
        (Char('j').into(), Func(cursor_down)),
        (Down.into(),      Func(cursor_down)),
        (Char('k').into(), Func(cursor_up)),
        (Up.into(),        Func(cursor_up)),
        (Char('l').into(), Func(cursor_right)),
        (Right.into(),     Func(cursor_right)),

        (Char('i').into(), Func(enter_insert_mode_at_cursor)),
        (Char('I').into(), Func(enter_insert_mode_at_first_non_whitespace)),
        (Char('a').into(), Func(enter_insert_mode_after_cursor)),
        (Char('A').into(), Func(enter_insert_mode_at_eol)),
        (Char('o').into(), Func(insert_line_below)),
        (Char('O').into(), Func(insert_line_above)),

        (Char('D').into(), Func(delete_until_eol)),
        (Char('C').into(), Func(change_until_eol)),

        (Char('X').into(), Func(delete_symbol_to_the_left)),
        (Char('d').into(), Map(d_keymap())),

        (Char('G').into(), Func(goto_last_line)),
        (Char('g').into(), Map(g_keymap())),
    ])
}

fn insert_mode_keymap() -> Keymap {
    use KeyCode::*;
    use Action::*;

    Keymap::from([
        (Esc.into(),   Func(enter_normal_mode)),

        (Left.into(),  Func(cursor_left)),
        (Down.into(),  Func(cursor_down)),
        (Up.into(),    Func(cursor_up)),
        (Right.into(), Func(cursor_right)),

        (Char('j').into(), Map(Keymap::from([(Char('k').into(), Func(enter_normal_mode))]))),

        (Backspace.into(), Func(delete_symbol_to_the_left)),

        (Enter.into(),     Func(append_new_line)),
    ])
}
