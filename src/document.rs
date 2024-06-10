use std::{borrow::Cow, path::PathBuf};

use crop::Rope;
use crate::editable_text::EditableText;

pub struct Document {
    pub text: EditableText,
    pub path: Option<PathBuf>,
    pub modified: bool,
    pub readonly: bool,
}

impl Document {
    pub fn new(data: Rope, path: Option<PathBuf>) -> Self {
        let readonly = match &path {
            Some(p) => {
                std::fs::metadata(p).is_ok_and(|m| {
                    m.permissions().readonly()
                })
            },
            None => false,
        };
        Self {
            text: EditableText::new(data),
            path,
            readonly,
            modified: false,
        }
    }

    pub fn filename(&self) -> Cow<'_, str> {
        match &self.path {
            Some(p) => match p.file_name() {
                Some(f) => f.to_string_lossy(),
                None => "[scratch]".into()
            }
            None => "[scratch]".into(),
        }
    }
}
