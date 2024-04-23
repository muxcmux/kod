use anyhow::Result;
use crossterm::{
    cursor,
    event::{read, Event, KeyCode, KeyEvent, KeyEventKind},
    style::{Color, Print},
    terminal, ExecutableCommand, QueueableCommand,
};
use std::{cmp::Ordering, env, io::Write};
use crop::Rope;

enum Mode {
    Normal,
    Insert,
}

enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    EnterNormalMode,
    EnterInsertModeAtCursor,
    EnterInsertModeAfterCursor,
    AppendCharacter(char),
    InsertLineBelow,
    InsertLineAbove,
    DeleteSymbolToTheLeft,
    Quit,
}

fn handle_key_event(mode: &Mode, event: &KeyEvent) -> Option<Action> {
    match mode {
        Mode::Normal => handle_normal_mode_key_event(event),
        Mode::Insert => handle_insert_mode_key_event(event),
    }
}

fn handle_normal_mode_key_event(event: &KeyEvent) -> Option<Action> {
    match event.code {
        KeyCode::Char('h') | KeyCode::Left => Some(Action::MoveLeft),
        KeyCode::Char('j') | KeyCode::Down=> Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('l') | KeyCode::Right => Some(Action::MoveRight),
        KeyCode::Char('i')=> Some(Action::EnterInsertModeAtCursor),
        KeyCode::Char('a') => Some(Action::EnterInsertModeAfterCursor),
        KeyCode::Char('o') => Some(Action::InsertLineBelow),
        KeyCode::Char('O') => Some(Action::InsertLineAbove),
        KeyCode::Char('q') => Some(Action::Quit),
        _ => None,
    }
}

fn handle_insert_mode_key_event(event: &KeyEvent) -> Option<Action> {
    match event.code {
        KeyCode::Esc => Some(Action::EnterNormalMode),
        KeyCode::Char(c) => Some(Action::AppendCharacter(c)),
        KeyCode::Enter => Some(Action::AppendCharacter('\n')),
        KeyCode::Backspace => Some(Action::DeleteSymbolToTheLeft),
        KeyCode::Left => Some(Action::MoveLeft),
        KeyCode::Down=> Some(Action::MoveDown),
        KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Right => Some(Action::MoveRight),
        _ => None,
    }
}

fn enter_screen() -> Result<()> {
    let mut stdout = std::io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        _ = std::io::stdout().execute(terminal::LeaveAlternateScreen);
        _ = terminal::disable_raw_mode();
        println!();
        default_panic(info);
    }));

    Ok(())
}

fn leave_screen() -> Result<()> {
    terminal::disable_raw_mode()?;
    std::io::stdout().execute(terminal::LeaveAlternateScreen)?;

    Ok(())
}

struct Renderer {
    stdout: std::io::Stdout,
    buffers: [RenderBuffer; 2],
    current: usize,
    width: usize,
    height: usize,
}

impl Renderer {
    fn default() -> Result<Self> {
        let stdout = std::io::stdout();

        let (width, height) = terminal::size()?;
        let width: usize = width.into();
        let height: usize = height.into();

        let buffers = [
            RenderBuffer::with_size(width, height),
            RenderBuffer::with_size(width, height)
        ];

        Ok(Self {
            stdout,
            buffers,
            current: 0,
            width,
            height,
        })
    }

    fn current_buffer_mut(&mut self) -> &mut RenderBuffer {
        &mut self.buffers[self.current]
    }

    fn draw(&mut self, state: &Editor) -> Result<()> {
        self.draw_buffer(state);
        // self.draw_statusline(state);
        self.paint()?;
        self.draw_cursor(state)?;
        self.stdout.flush()?;
        Ok(())
    }

    fn paint(&mut self) -> Result<()> {
        let prev_buffer = &self.buffers[1 - self.current];
        let curr_buffer = &self.buffers[self.current];

        for Patch { cell: Cell { symbol, fg, bg, }, x, y, } in prev_buffer.diff(curr_buffer) {
            self.stdout.queue(cursor::MoveTo(x as u16, y as u16))?;
            self.stdout.queue(Print(&symbol))?;
        }

        self.buffers[1 - self.current].reset();
        self.current = 1 - self.current;

        Ok(())
    }

    fn draw_buffer(&mut self, state: &Editor) {
        for row in state.buffer_view.row_range() {
            if row >= state.text_buffer.rope.lines().len() {
                break;
            }
            let line = state.text_buffer.rope.line(row);
            let mut graphemes = line.graphemes();
            let mut skip_next_n_cols = 0;
            // advance the iterator
            for _ in 0..state.buffer_view.scroll_x { graphemes.next(); }
            for col in state.buffer_view.col_range() {
                if skip_next_n_cols > 0 {
                    skip_next_n_cols -= 1;
                    continue;
                }
                match graphemes.next() {
                    None => break,
                    Some(g) => {
                        let width = unicode_display_width::width(&g);
                        let x = col.saturating_sub(state.buffer_view.scroll_x);
                        let y = row.saturating_sub(state.buffer_view.scroll_y);
                        self.current_buffer_mut().put_symbol(g.to_string(), width as u8, x, y, None, None);
                        skip_next_n_cols = width - 1;
                    }
                }
            }
        }
    }

    // fn draw_statusline(&mut self, state: &Editor) {
    //     let label = match state.mode {
    //         Mode::Normal => " NOR ",
    //         Mode::Insert => " INS ",
    //     };

    //     let (cursor_x, cursor_y) = state.buffer_view.cursor_position_relative_to_buffer();
    //     let position = format!(" {}:{} ", cursor_y + 1, cursor_x + 1);

    //     if label.len() + position.len() < self.width {
    //         let chars: Vec<char> = label.chars().collect();
    //         self.curr_buffer.put_chars(
    //             &chars,
    //             0,
    //             self.height - 1,
    //             Some(Color::Black),
    //             Some(Color::White),
    //         );
    //         for x in label.len()..self.width - position.len() {
    //             self.curr_buffer
    //                 .put_symbol(' ', x, self.height - 1, None, None);
    //         }
    //         let position_chars: Vec<char> = position.chars().collect();
    //         self.curr_buffer.put_chars(
    //             &position_chars,
    //             self.width - position.len(),
    //             self.height - 1,
    //             Some(Color::Black),
    //             Some(Color::White),
    //         );
    //     }
    // }

    fn draw_cursor(&mut self, state: &Editor) -> Result<()> {
        let x = state.buffer_view.cursor_x as u16;
        let y = state.buffer_view.cursor_y as u16;
        self.stdout.queue(cursor::MoveTo(x, y))?;

        match state.mode {
            Mode::Normal => self.stdout.queue(cursor::SetCursorStyle::SteadyBlock)?,
            Mode::Insert => self.stdout.queue(cursor::SetCursorStyle::SteadyBar)?,
        };

        Ok(())
    }
}

struct TextBuffer {
    rope: Rope,
    cursor_x: usize,
    cursor_y: usize,
    sticky_cursor_x: usize,
    last_vertical_move_dir: Option<MovementDirection>,
    last_horizontal_move_dir: Option<MovementDirection>,
    // cache - do we *really* need this?
    rows: usize,
    cols: Vec<usize>,
}

impl TextBuffer {
    fn new(rope: Rope) -> Self {
        let mut cols = Vec::with_capacity(rope.lines().len());
        let mut rows = 0;

        for (i, line) in rope.lines().enumerate() {
            cols.push(line.graphemes().map(|g| unicode_display_width::width(&g) as usize).sum());
            rows = i + 1;
        }

        Self { rope, rows, cols, cursor_x: 0, cursor_y: 0, sticky_cursor_x: 0, last_vertical_move_dir: None, last_horizontal_move_dir: None }
    }

    fn byte_offset_at_cursor(&self, cursor_x: usize, cursor_y: usize) -> usize {
        let mut offset = self.rope.byte_of_line(cursor_y);
        let mut col = 0;
        for g in self.rope.line(cursor_y).graphemes() {
            if col == cursor_x {
                break;
            }
            col += unicode_display_width::width(&g) as usize;
            offset += g.len();
        }
        offset
    }

    fn insert_char_at_cursor(&mut self, char: char, mode: &Mode) {
        let offset = self.byte_offset_at_cursor(self.cursor_x, self.cursor_y);
        let mut buf = [0; 4];
        let text = char.encode_utf8(&mut buf);
        let text_width = unicode_display_width::width(text) as usize;

        self.rope.insert(offset, text);

        if char == '\n' {
            let mut old_line_count = 0;
            let mut new_line_count = 0;
            for g in self.rope.line(self.cursor_y).graphemes() {
                old_line_count += unicode_display_width::width(&g) as usize;
            }
            for g in self.rope.line(self.cursor_y + 1).graphemes() {
                new_line_count += unicode_display_width::width(&g) as usize;
            }
            self.cols[self.cursor_y] = old_line_count;
            self.cols.insert(self.cursor_y + 1, new_line_count);
            self.rows += 1;

            self.move_cursor_to(Some(0), Some(self.cursor_y + 1), mode);
        } else {
            self.cols[self.cursor_y] += text_width;

            self.cursor_right(mode);
        }
    }

    fn move_cursor_to(&mut self, x: Option<usize>, y: Option<usize>, mode: &Mode) {
        // ensure x and y are within bounds
        let y = self.rows.saturating_sub(1).min(y.unwrap_or(self.cursor_y));
        let max_x = match mode {
            Mode::Insert => self.cols[y],
            Mode::Normal => self.cols[y].saturating_sub(1),
        };
        let x = max_x.min(x.unwrap_or(self.sticky_cursor_x));

        self.last_horizontal_move_dir = match self.cursor_x.cmp(&x) {
            Ordering::Greater => Some(MovementDirection::Backward),
            Ordering::Less => Some(MovementDirection::Forward),
            Ordering::Equal => None,
        };

        self.last_vertical_move_dir = match self.cursor_y.cmp(&y) {
            Ordering::Greater => Some(MovementDirection::Backward),
            Ordering::Less => Some(MovementDirection::Forward),
            Ordering::Equal => None,
        };

        self.cursor_x = x;
        self.cursor_y = y;
    }

    fn cursor_up(&mut self, mode: &Mode) {
        self.move_cursor_to(
            None,
            Some(self.cursor_y.saturating_sub(1)),
            mode
        );
    }

    fn cursor_down(&mut self, mode: &Mode) {
        self.move_cursor_to(
            None,
            Some(self.cursor_y + 1),
            mode
        );
    }

    fn cursor_left(&mut self, mode: &Mode) {
        self.move_cursor_to(
            Some(self.cursor_x.saturating_sub(1)),
            None,
            mode
        );

        self.sticky_cursor_x = self.cursor_x;
    }

    fn cursor_right(&mut self, mode: &Mode) {
        self.move_cursor_to(
            Some(self.cursor_x + 1),
            None,
            mode
        );

        self.sticky_cursor_x = self.cursor_x;
    }

}

enum MovementDirection {
    Forward,
    Backward,
}

struct ScrollView {
    width: usize,
    height: usize,
    cursor_x: usize,
    cursor_y: usize,
    offset_x: usize,
    offset_y: usize,
    scroll_x: usize,
    scroll_y: usize,
}

impl ScrollView {
    fn with_size(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            offset_x: 20,
            offset_y: 2,
            scroll_y: 0,
            scroll_x: 0,
        }
    }

    fn row_range(&self) -> std::ops::Range<usize> {
        self.scroll_y..self.scroll_y + self.height
    }

    fn col_range(&self) -> std::ops::Range<usize> {
        self.scroll_x..self.scroll_x + self.width
    }

    fn scroll_cursor_into_view(&mut self, buffer: &TextBuffer) {
        if let Some(ref dir) = buffer.last_vertical_move_dir {
            match dir {
                MovementDirection::Backward => self.scroll_up(buffer),
                MovementDirection::Forward => self.scroll_down(buffer),
            }
        }

        if let Some(ref dir) = buffer.last_horizontal_move_dir {
            match dir {
                MovementDirection::Backward => self.scroll_left(buffer),
                MovementDirection::Forward => self.scroll_right(buffer),
            }
        }
    }

    fn scroll_up(&mut self, buffer: &TextBuffer) {
        self.scroll_y = buffer.cursor_y.saturating_sub(self.offset_y).min(self.scroll_y);

        self.cursor_y = buffer.cursor_y.saturating_sub(self.scroll_y);
    }

    fn scroll_down(&mut self, buffer: &TextBuffer) {
        let max_scroll_y = buffer.rows.saturating_sub(self.height);
        let scroll_y = buffer.cursor_y.saturating_sub(self.height.saturating_sub(self.offset_y + 1)).min(max_scroll_y);
        self.scroll_y = self.scroll_y.max(scroll_y);

        self.cursor_y = buffer.cursor_y.saturating_sub(self.scroll_y);
    }

    fn scroll_left(&mut self, buffer: &TextBuffer) {
        let scroll_x = buffer.cursor_x.saturating_sub(self.offset_x).min(self.scroll_x);
        self.scroll_x = scroll_x;

        self.cursor_x = buffer.cursor_x.saturating_sub(self.scroll_x);
    }

    fn scroll_right(&mut self, buffer: &TextBuffer) {
        let max_scroll_x = buffer.cols.iter().max().unwrap_or(&0).saturating_sub(self.width);
        let scroll_x = buffer.cursor_x.saturating_sub(self.width.saturating_sub(self.offset_x + 1)).min(max_scroll_x);
        self.scroll_x = self.scroll_x.max(scroll_x);

        self.cursor_x = buffer.cursor_x.saturating_sub(self.scroll_x);
    }
}

struct Editor {
    mode: Mode,
    buffer_view: ScrollView,
    text_buffer: TextBuffer,
}

impl Editor {
    fn with_size(width: usize, height: usize) -> Result<Self> {
        let mut args: Vec<String> = env::args().collect();

        let mut text_buffer = TextBuffer::new(Rope::from("\n"));
        if args.len() > 1 {
            let contents = std::fs::read_to_string(args.pop().unwrap())?;
            text_buffer = TextBuffer::new(Rope::from(contents));
        }

        Ok(Self {
            text_buffer,
            mode: Mode::Normal,
            buffer_view: ScrollView::with_size(width, height),
        })
    }

    fn enter_normal_mode(&mut self) {
        self.mode = Mode::Normal;
        self.text_buffer.cursor_left(&self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }

    fn enter_insert_mode_relative_to_cursor(&mut self, x: usize) {
        self.mode = Mode::Insert;
        for _ in 0..x {
            self.text_buffer.cursor_right(&self.mode);
            self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
        }
    }

    fn append_character(&mut self, c: char) {
        self.text_buffer.insert_char_at_cursor(c, &self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }

    fn cursor_up(&mut self) {
        self.text_buffer.cursor_up(&self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }

    fn cursor_down(&mut self) {
        self.text_buffer.cursor_down(&self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }

    fn cursor_left(&mut self) {
        self.text_buffer.cursor_left(&self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }

    fn cursor_right(&mut self) {
        self.text_buffer.cursor_right(&self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }

    fn insert_line_below(&mut self) {
        self.mode = Mode::Insert;
        self.text_buffer.move_cursor_to(Some(std::usize::MAX), None, &self.mode);
        self.text_buffer.insert_char_at_cursor('\n', &self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }

    fn insert_line_above(&mut self) {
        self.mode = Mode::Insert;
        self.text_buffer.cursor_up(&self.mode);
        self.text_buffer.move_cursor_to(Some(std::usize::MAX), None, &self.mode);
        self.text_buffer.insert_char_at_cursor('\n', &self.mode);
        self.buffer_view.scroll_cursor_into_view(&self.text_buffer);
    }
}

#[derive(PartialEq, Debug, Clone)]
struct Cell {
    symbol: String,
    fg: Option<Color>,
    bg: Option<Color>,
}

impl Cell {
    fn empty() -> Self {
        Self {
            symbol: " ".to_string(),
            fg: None,
            bg: None,
        }
    }

    fn set_symbol(&mut self, symbol: &str) {
        self.symbol.clear();
        self.symbol.push_str(symbol);
    }

    fn reset(&mut self) {
        self.set_symbol(" ");
    }
}

#[derive(Debug)]
struct Patch<'a> {
    cell: &'a Cell,
    x: usize,
    y: usize,
}

#[derive(Clone, Debug)]
struct RenderBuffer {
    cells: Vec<Cell>,
    width: usize,
    height: usize,
}

impl RenderBuffer {
    fn with_size(width: usize, height: usize) -> Self {
        let cells = vec![Cell::empty(); width * height];
        Self {
            width,
            height,
            cells,
        }
    }

    fn reset(&mut self) {
        for cell in &mut self.cells {
            cell.reset();
        }
    }

    fn diff<'a>(&'a self, other: &'a Self) -> Vec<Patch> {
        assert!(self.width == other.width && self.height == other.height);

        self.cells
            .iter()
            .zip(other.cells.iter())
            .enumerate()
            .filter(|(_, (a, b))| *a != *b)
            .map(|(i, (_, cell))| Patch {
                cell,
                x: i % self.width,
                y: i / self.width,
            })
            .collect()
    }

    // fn resize(&mut self, width: usize, height: usize) {
    //     self.width = width;
    //     self.height = height;
    //     self.cells.resize(width * height, Cell::empty());
    // }

    fn put_symbol(&mut self, symbol: String, _width: u8, x: usize, y: usize, fg: Option<Color>, bg: Option<Color>) {
        let index = self.width * y + x;

        if let Some(cell) = self.cells.get_mut(index) {
            *cell = Cell { symbol, fg, bg };
        }
    }

    // fn put_chars(&mut self, chars: &[char], x: usize, y: usize, fg: Option<Color>, bg: Option<Color>) {
    //     let start = self.width * y + x;

    //     for (offset, &char) in chars.iter().enumerate() {
    //         if start + offset > self.cells.len() {
    //             break;
    //         }
    //         if let Some(cell) = self.cells.get_mut(start + offset) {
    //             *cell = Cell { symbol: char, fg, bg };
    //         }
    //     }
    // }
}

fn setup_logging() -> Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| out.finish(format_args!("{}: {}", record.level(), message)))
        .level(log::LevelFilter::Debug)
        .chain(fern::log_file("log.log")?)
        .apply()?;

    Ok(())
}

fn main() -> Result<()> {
    setup_logging()?;
    enter_screen()?;

    let mut renderer = Renderer::default()?;

    let mut editor = Editor::with_size(renderer.width, renderer.height.saturating_sub(1))?;

    loop {
        renderer.draw(&editor)?;

        match read()? {
            Event::Key(event) => {
                if event.kind == KeyEventKind::Press {
                    if let Some(action) = handle_key_event(&editor.mode, &event) {
                        match action {
                            Action::Quit => break,
                            Action::MoveUp => {
                                editor.cursor_up();
                            }
                            Action::MoveDown => {
                                editor.cursor_down();
                            }
                            Action::MoveLeft => {
                                editor.cursor_left();
                            }
                            Action::MoveRight => {
                                editor.cursor_right();
                            }
                            Action::EnterInsertModeAtCursor => {
                                editor.enter_insert_mode_relative_to_cursor(0);
                            }
                            Action::EnterInsertModeAfterCursor => {
                                editor.enter_insert_mode_relative_to_cursor(1);
                            }
                            Action::EnterNormalMode => {
                                editor.enter_normal_mode();
                            }
                            Action::InsertLineBelow => {
                                editor.insert_line_below();
                            }
                            Action::InsertLineAbove => {
                                editor.insert_line_above();
                            }
                            Action::AppendCharacter(c) => {
                                editor.append_character(c);
                            }
                            Action::DeleteSymbolToTheLeft => {
                                // editor.delete_symbol_to_the_left();
                            }
                        }
                    }
                }
            }
            Event::Resize(_, _) => todo!(),
            Event::Mouse(_) => todo!(),
            Event::Paste(_) => todo!(),
            Event::FocusGained => todo!(),
            Event::FocusLost => todo!(),
        }
    }

    leave_screen()?;
    Ok(())
}
