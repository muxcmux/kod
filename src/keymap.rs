use std::collections::HashMap;

use crossterm::event::KeyCode;
use crate::{commands::*, editor::Mode};

type Command = fn(&mut Context);
type Keymap = HashMap<KeyCode, Action>;

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

        // if the action is a command, or the key isn't mapped,
        // short circuit and return a result with the command or not found
        let action = match keymap.get(root) {
            None => { return KeymapResult::NotFound },
            Some(Action::Command(command)) => { return KeymapResult::Found(*command) }
            Some(keymap) => keymap,
        };

        // otherwise push the current key code to the pending keys
        self.pending.push(key);

        // and search for an action in this action's keymap
        match action.find_by_path(&self.pending[1..]) {
            None => KeymapResult::Cancelled(self.pending.drain(..).collect()),
            Some(Action::Keymap(_)) => KeymapResult::Pending,
            Some(Action::Command(command)) => {
                self.pending.clear();
                KeymapResult::Found(*command)
            }
        }
    }
}

#[derive(Clone)]
pub enum Action {
    Command(Command),
    Keymap(Keymap)
}

impl Action {
    pub fn find_by_path(&self, path: &[KeyCode]) -> Option<&Self> {
        let mut current = self;

        for key in path {
            current = match current {
                Action::Keymap(map) => map.get(key),
                Action::Command(_) => None,
            }?
        }

        Some(current)
    }
}

pub enum KeymapResult {
    Found(Command),
    Pending,
    Cancelled(Vec<KeyCode>),
    NotFound,
}

fn g_keymap() -> Keymap {
    let mut map = Keymap::new();
    map.insert(KeyCode::Char('g'), Action::Command(goto_first_line));

    map
}

fn normal_mode_keymap() -> Keymap {
    let mut map = Keymap::new();
    map.insert(KeyCode::Char('h'), Action::Command(cursor_left));
    map.insert(KeyCode::Left, Action::Command(cursor_left));
    map.insert(KeyCode::Char('j'), Action::Command(cursor_down));
    map.insert(KeyCode::Down, Action::Command(cursor_down));
    map.insert(KeyCode::Char('k'), Action::Command(cursor_up));
    map.insert(KeyCode::Up, Action::Command(cursor_up));
    map.insert(KeyCode::Char('l'), Action::Command(cursor_right));
    map.insert(KeyCode::Right, Action::Command(cursor_right));

    map.insert(KeyCode::Char('i'), Action::Command(enter_insert_mode_at_cursor));
    map.insert(KeyCode::Char('a'), Action::Command(enter_insert_mode_after_cursor));
    map.insert(KeyCode::Char('A'), Action::Command(enter_insert_mode_at_eol));
    map.insert(KeyCode::Char('o'), Action::Command(insert_line_below));
    map.insert(KeyCode::Char('O'), Action::Command(insert_line_above));

    map.insert(KeyCode::Char('G'), Action::Command(goto_last_line));
    map.insert(KeyCode::Char('g'), Action::Keymap(g_keymap()));

    map.insert(KeyCode::Char('q'), Action::Command(quit));
    map.insert(KeyCode::Char('s'), Action::Command(save));

    map
}

fn insert_mode_keymap() -> Keymap {
    let mut map = Keymap::new();
    map.insert(KeyCode::Esc, Action::Command(enter_normal_mode));

    map.insert(KeyCode::Left, Action::Command(cursor_left));
    map.insert(KeyCode::Down, Action::Command(cursor_down));
    map.insert(KeyCode::Up, Action::Command(cursor_up));
    map.insert(KeyCode::Right, Action::Command(cursor_right));

    map.insert(KeyCode::Char('j'), Action::Keymap(Keymap::from([(KeyCode::Char('k'), Action::Command(enter_normal_mode))])));

    map.insert(KeyCode::Backspace, Action::Command(delete_symbol_to_the_left));
    map
}
