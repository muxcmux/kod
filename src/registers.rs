use std::collections::HashMap;

#[derive(Default)]
pub struct Registers {
    // selected: Option<char>,
    map: HashMap<char, Vec<String>>
}

impl Registers {
    pub fn get(&self, reg: char) -> Option<&Vec<String>> {
        self.map.get(&reg)
    }

    pub fn push(&mut self, reg: char, value: String) {
        match self.map.get_mut(&reg) {
            Some(contents) => {
                if contents.last().is_none_or(|c| c != &value) {
                    contents.push(value);
                }
            }
            None => {
                self.map.insert(reg, vec![value]);
            },
        }
    }

    pub fn get_nth(&self, reg: char, idx: usize) -> Option<&String> {
        self.get(reg).and_then(|r| r.get(idx))
    }
}
