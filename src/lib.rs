use std::num::NonZeroIsize;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct IncrementalId(NonZeroIsize);

impl Default for IncrementalId {
    fn default() -> Self {
        Self(NonZeroIsize::new(1).unwrap())
    }
}

impl IncrementalId {
    // return the next id
    fn next(&self) -> Self {
        Self(NonZeroIsize::new(self.0.get() + 1).unwrap())
    }

    // return the current id and advance it
    fn advance(&mut self) -> Self {
        let current = self.clone();
        *self = self.next();
        current
    }
}

pub mod application;
mod history;
mod components;
mod commands;
mod compositor;
mod document;
mod editor;
mod keymap;
mod ui;
mod panes;
mod graphemes;
