pub struct RopeCursor<'a> {
    slices: Vec<(usize, &'a str)>,
    total_slices: usize,
    index: usize,
    total_bytes: usize,
}

impl<'a> RopeCursor<'a> {
    pub fn new(rope: &'a crop::Rope) -> Self {
        let mut slices: Vec<(usize, &str)> = vec![];
        let mut offset = 0;

        for chunk in rope.chunks() {
            slices.push((offset, chunk));
            offset += chunk.len();
        }

        let total_slices = slices.len();

        Self { slices, total_slices, index: 0, total_bytes: offset }
    }
}

impl regex_cursor::Cursor for RopeCursor<'_> {
    fn chunk(&self) -> &[u8] {
        let chunk = self.slices[self.index].1;
        chunk.as_bytes()
    }

    fn advance(&mut self) -> bool {
        if self.index == self.total_slices.saturating_sub(1) {
            return false
        }

        self.index += 1;
        true
    }

    fn backtrack(&mut self) -> bool {
        if self.index == 0 {
            return false
        }

        self.index -= 1;
        true
    }

    fn total_bytes(&self) -> Option<usize> {
        Some(self.total_bytes)
    }

    fn offset(&self) -> usize {
        self.slices[self.index].0
    }
}
