macro_rules! map {
    (@action $func:ident) => {
        $crate::keymap::Action::Func($crate::commands::actions::$func)
    };

    (@action
        { $($($key:literal)|+ => $value:tt,)+ }
    ) => {
        $crate::keymap::Action::Map(map!({ $($($key)|+ => $value,)+ }))
    };

    (
        { $($($key:literal)|+ => $value:tt,)+ }
    ) => {
        {
            let mut map = $crate::keymap::Keymap::new();
            $(
                $(
                    let key = $crate::keymap::parse_key_combo($key);
                    let duplicate = map.insert(key, map!(@action $value));
                    debug_assert!(duplicate.is_none(), "Duplicate key combo: {}", stringify!($key));
                )+
            )*
            map
        }
    };
}

pub mod default;
pub(crate) use map;

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use once_cell::sync::Lazy;
use crate::{commands::{self, actions::ActionResult}, editor::Mode};

type Func = fn(&mut commands::Context) -> ActionResult;
type Keymap = HashMap<KeyEvent, Action>;

#[derive(Debug)]
pub struct Keymaps {
    map: HashMap<Mode, Keymap>,
    pending: Vec<KeyEvent>,
}

impl Default for Keymaps {
    fn default() -> Self {
        let mut map = HashMap::new();
        use default::*;
        map.insert(Mode::Normal, normal_mode_keymap());
        map.insert(Mode::Insert, insert_mode_keymap());
        map.insert(Mode::Replace, replace_mode_keymap());
        map.insert(Mode::Select, select_mode_keymap());

        Self { map, pending: vec![] }
    }
}

impl Keymaps {
    pub fn get(&mut self, mode: &Mode, event: KeyEvent) -> KeymapResult {
        // gets the keymap for the mode
        let keymap = self.map.get(mode).unwrap_or_else(|| panic!("No keymap found for editor mode {:?}", mode));

        // esc key clears the pending keys and returns a cancelled
        // event with the current pending keys, so they can be
        // used elsewhere
        if event.code == KeyCode::Esc && !self.pending.is_empty() {
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

static KEYS: Lazy<HashMap<&str, KeyCode>> = Lazy::new(|| {
    HashMap::from([
        ("minus", KeyCode::Char('-')),
        ("space", KeyCode::Char(' ')),
        ("backspace", KeyCode::Backspace),
        ("enter", KeyCode::Enter),
        ("left", KeyCode::Left),
        ("right", KeyCode::Right),
        ("up", KeyCode::Up),
        ("down", KeyCode::Down),
        ("home", KeyCode::Home),
        ("end", KeyCode::End),
        ("pageup", KeyCode::PageUp),
        ("pagedown", KeyCode::PageDown),
        ("tab", KeyCode::Tab),
        ("backtab", KeyCode::BackTab),
        ("delete", KeyCode::Delete),
        ("insert", KeyCode::Insert),
        ("null", KeyCode::Null),
        ("esc", KeyCode::Esc),
        ("capslock", KeyCode::CapsLock),
        ("scrolllock", KeyCode::ScrollLock),
        ("numlock", KeyCode::NumLock),
        ("printscreen", KeyCode::PrintScreen),
        ("pause", KeyCode::Pause),
        ("menu", KeyCode::Menu),
        ("keypadbegin", KeyCode::KeypadBegin),
        // ("Media(_)", KeyCode::Media(_)),
    ])
});

fn parse_key_combo(combo: &str) -> KeyEvent {
    let mut tokens: Vec<&str> = combo.split('-').collect();
    let mut key_code = match tokens.pop().expect("Key combo cannot be empty") {
        c if c.chars().count() == 1 => KeyCode::Char(c.chars().next().unwrap()),
        fun if fun.chars().count() > 1 && fun.starts_with('F') => {
            let number: u8 = fun.chars().skip(1).collect::<String>().parse().expect("Invalid function key combo");
            debug_assert!(number > 0 && number < 25, "Invalid function key combo: F{number}");
            KeyCode::F(number)
        }
        other if KEYS.get(other).is_some() => *KEYS.get(other).unwrap(),
        invalid => panic!("Invalid key combo: {invalid}"),
    };

    let mut modifiers = KeyModifiers::empty();

    for token in tokens {
        let modifier = match token {
            "S" => KeyModifiers::SHIFT,
            "A" => KeyModifiers::ALT,
            "C" => KeyModifiers::CONTROL,
            _ => panic!("Invalid key modifier '{}-'", token),
        };

        debug_assert!(!modifiers.contains(modifier), "Repeated key modifier '{token}-'");
        modifiers.insert(modifier);
    }

    if let KeyCode::Char(c) = key_code {
        if c.is_ascii_lowercase() && modifiers.contains(KeyModifiers::SHIFT) {
            key_code = KeyCode::Char(c.to_ascii_uppercase());
            modifiers.remove(KeyModifiers::SHIFT);
        }
    }

    KeyEvent::new(key_code, modifiers)
}
