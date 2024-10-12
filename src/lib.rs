use std::num::NonZeroUsize;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NonZeroIncrementalId(NonZeroUsize);

impl Default for NonZeroIncrementalId {
    fn default() -> Self {
        Self(NonZeroUsize::new(1).unwrap())
    }
}

impl NonZeroIncrementalId {
    fn next(&self) -> Self {
        Self(NonZeroUsize::new(self.0.get() + 1).unwrap())
    }

    fn advance(&mut self) {
        self.0 = NonZeroUsize::new(self.0.get() + 1).unwrap();
    }
}

pub mod application;
mod history;
mod components;
mod commands;
mod compositor;
mod document;
mod editable_text;
mod editor;
mod keymap;
mod ui;
mod panes;
