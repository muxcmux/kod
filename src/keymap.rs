macro_rules! key {
    ($key:ident) => {
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::$key,
            crossterm::event::KeyModifiers::NONE
        )
    };
    ($ch:tt) => {
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char($ch),
            crossterm::event::KeyModifiers::NONE
        )
    };
}

macro_rules! map {
    (@action $func:ident) => {
        $crate::keymap::Action::Func($func)
    };

    (@action
        { $($($key:tt)|+ => $value:tt,)+ }
    ) => {
        $crate::keymap::Action::Map(map!({ $($($key)|+ => $value,)+ }))
    };

    (
        { $($($key:tt)|+ => $value:tt,)+ }
    ) => {
        {
            let mut map = $crate::keymap::Keymap::new();
            $(
                $(
                    let key = key!($key);
                    let duplicate = map.insert(key, map!(@action $value));
                    assert!(duplicate.is_none(), "Duplicate key found: {}", stringify!($key));
                )+
            )*
            map
        }
    };
}

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

fn normal_mode_keymap() -> Keymap {
    map!({
        'h' | Left => cursor_left,
        'j' | Down => cursor_down,
        'k' | Up => cursor_up,
        'l' | Right => cursor_right,

        'i' => enter_insert_mode_at_cursor,
        'I' => enter_insert_mode_at_first_non_whitespace,
        'a' => enter_insert_mode_after_cursor,
        'A' => enter_insert_mode_at_eol,
        'o' => insert_line_below,
        'O' => insert_line_above,

        'D' => delete_until_eol,
        'C' => change_until_eol,

        'X' => delete_symbol_to_the_left,
        'd' =>  {
            'd' => delete_current_line,
        },

        'G' => goto_last_line,

        'g' => {
            'g' => goto_first_line,
        },
    })
}

fn insert_mode_keymap() -> Keymap {
    map!({
        Esc => enter_normal_mode,

        Left => cursor_left,
        Down => cursor_down,
        Up => cursor_up,
        Right => cursor_right,

        'j' => {
            'k' => enter_normal_mode,
        },

        Backspace => delete_symbol_to_the_left,

        Enter => append_new_line,
    })
}
