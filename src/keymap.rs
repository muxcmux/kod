use std::collections::HashMap;

use crossterm::event::KeyCode;
use crate::{actions::*, editor::Mode};

type Func = fn(&mut Context);
type Keymap = HashMap<KeyCode, Action>;

#[derive(Debug)]
pub struct Keymaps {
    map: HashMap<Mode, Keymap>,
    pending: Vec<KeyCode>,
}

impl Keymaps {
    pub fn default() -> Self {
        let mut map = HashMap::new();
        map.insert(Mode::Normal, normal_mode_keymap());
        map.insert(Mode::Insert, insert_mode_keymap());

        Self { map, pending: vec![] }
    }

    pub fn get(&mut self, mode: &Mode, key: KeyCode) -> KeymapResult {
        // gets the keymap for the mode
        let keymap = self.map.get(mode).unwrap_or_else(|| panic!("No keymap found for editor mode {:?}", mode));

        // esc key clears the pending keys and returns a cancelled
        // event with the current pending keys, so they can be
        // used elsewhere
        if matches!(key, KeyCode::Esc) && !self.pending.is_empty() {
            return KeymapResult::Cancelled(self.pending.drain(..).collect());
        }

        // get the action for the root key in the keymap
        let root = self.pending.first().unwrap_or(&key);

        // if the action is a function, or the key isn't mapped,
        // short circuit and return a result with the function or not found
        let action = match keymap.get(root) {
            None => { return KeymapResult::NotFound },
            Some(Action::Func(f)) => { return KeymapResult::Found(*f) }
            Some(keymap) => keymap,
        };

        // otherwise push the current key code to the pending keys
        self.pending.push(key);

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
    pub fn find_by_path(&self, path: &[KeyCode]) -> Option<&Self> {
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
    Cancelled(Vec<KeyCode>),
    NotFound,
}

fn g_keymap() -> Keymap {
    let mut map = Keymap::new();
    map.insert(KeyCode::Char('g'), Action::Func(goto_first_line));

    map
}

fn d_keymap() -> Keymap {
    use KeyCode::*;
    use Action::*;

    Keymap::from([
        (Char('d'), Func(delete_current_line))
    ])
}

fn normal_mode_keymap() -> Keymap {
    use KeyCode::*;
    use Action::*;

    Keymap::from([
        (Char('h'), Func(cursor_left)),
        (Left,      Func(cursor_left)),
        (Char('j'), Func(cursor_down)),
        (Down,      Func(cursor_down)),
        (Char('k'), Func(cursor_up)),
        (Up,        Func(cursor_up)),
        (Char('l'), Func(cursor_right)),
        (Right,     Func(cursor_right)),

        (Char('i'), Func(enter_insert_mode_at_cursor)),
        (Char('I'), Func(enter_insert_mode_at_first_non_whitespace)),
        (Char('a'), Func(enter_insert_mode_after_cursor)),
        (Char('A'), Func(enter_insert_mode_at_eol)),
        (Char('o'), Func(insert_line_below)),
        (Char('O'), Func(insert_line_above)),

        (Char('D'), Func(delete_until_eol)),
        (Char('C'), Func(change_until_eol)),

        (Char('X'), Func(delete_symbol_to_the_left)),
        (Char('d'), Map(d_keymap())),

        (Char('G'), Func(goto_last_line)),
        (Char('g'), Map(g_keymap())),
    ])
}

fn insert_mode_keymap() -> Keymap {
    use KeyCode::*;
    use Action::*;

    Keymap::from([
        (Esc,   Func(enter_normal_mode)),

        (Left,  Func(cursor_left)),
        (Down,  Func(cursor_down)),
        (Up,    Func(cursor_up)),
        (Right, Func(cursor_right)),

        (Char('j'), Map(Keymap::from([(Char('k'), Func(enter_normal_mode))]))),

        (Backspace, Func(delete_symbol_to_the_left)),

        (Enter,     Func(append_new_line)),
    ])
}
