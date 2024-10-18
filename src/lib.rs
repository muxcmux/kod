use std::{fmt::{Debug, Write}, num::NonZeroIsize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
        let current = *self;
        *self = self.next();
        current
    }
}

impl Debug for IncrementalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
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
