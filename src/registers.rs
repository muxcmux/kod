use std::collections::HashMap;

#[derive(Default)]
pub struct Registers {
    selected: Option<char>,
    map: HashMap<char, String>
}

impl Registers {
    pub fn read(&self, reg: char) -> Option<&str> {
        self.map.get(&reg).map(|x| x.as_str())
    }

    pub fn write(&mut self, reg: char, value: String) {
        self.map.insert(reg, value);
    }
}
