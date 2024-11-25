macro_rules! make_inc_id_type {
    ($type:ident) => {
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $type(std::num::NonZeroIsize);

        impl Default for $type {
            fn default() -> Self {
                Self(std::num::NonZeroIsize::new(1).unwrap())
            }
        }

        impl $type {
            // return the next id
            fn next(&self) -> Self {
                Self(std::num::NonZeroIsize::new(self.0.get() + 1).unwrap())
            }

            // return the current id and advance it
            fn advance(&mut self) -> Self {
                let current = *self;
                *self = self.next();
                current
            }
        }

        impl std::fmt::Debug for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
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
mod gutter;
mod search;
mod registers;
mod rope;
mod syntax;
