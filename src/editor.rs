use std::{env, fs, path::PathBuf};

use anyhow::Result;
use crop::Rope;

use crate::document::Document;

#[derive(Eq, Hash, PartialEq, Debug)]
pub enum Mode {
    Normal,
    Insert,
}

pub struct Editor {
    pub mode: Mode,
    pub document: Document,
    pub quit: bool,
}

impl Editor {
    pub fn new() -> Result<Self> {
        let mut args: Vec<String> = env::args().collect();

        let mut path = None;
        let data = if args.len() > 1 {
            let pa = PathBuf::from(args.pop().unwrap());
            let contents = std::fs::read_to_string(&pa)?;
            path = Some(pa);
            Rope::from(if contents.is_empty() { "\n" } else { &contents })
        } else {
            Rope::from("\n")
        };

        let document = Document::new(data, path);

        Ok(Self {
            document,
            mode: Mode::Normal,
            quit: false,
        })
    }

    pub fn save_document(&mut self) {
        if let Some(path) = &self.document.path {
            fs::write(path, self.document.data.to_string()).expect("couldn't write to file");
        }
        self.document.modified = false;
    }
}

