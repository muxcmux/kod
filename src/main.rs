use anyhow::Result;
use crossterm::{
    cursor,
    event::{read, Event, KeyCode, KeyEvent, KeyEventKind},
    style::{self, Color, Stylize},
    terminal, ExecutableCommand, QueueableCommand,
};
use log::debug;
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
    NewLine,
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
        KeyCode::Char('h') => Some(Action::MoveLeft),
        KeyCode::Char('j') => Some(Action::MoveDown),
        KeyCode::Char('k') => Some(Action::MoveUp),
        KeyCode::Char('l') => Some(Action::MoveRight),
        KeyCode::Char('i') => Some(Action::EnterInsertModeAtCursor),
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
        KeyCode::Enter => Some(Action::NewLine),
        KeyCode::Backspace => Some(Action::DeleteSymbolToTheLeft),
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
    curr_buffer: RenderBuffer,
    prev_buffer: RenderBuffer,
    width: usize,
    height: usize,
}

impl Renderer {
    fn default() -> Result<Self> {
        let stdout = std::io::stdout();

        let (width, height) = terminal::size()?;
        let width: usize = width.into();
        let height: usize = height.into();

        let curr_buffer = RenderBuffer::with_size(width, height);
        let prev_buffer = RenderBuffer::with_size(width, height);

        Ok(Self {
            stdout,
            curr_buffer,
            prev_buffer,
            width,
            height,
        })
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
        // log::debug!("Buffer:");
        // for (i, cell) in self.curr_buffer.cells.iter().enumerate() {
        //     if cell.symbol != " " {
        //         let x = i % self.width;
        //         let y = i / self.width;
        //         log::debug!("{x}, {y}: {}", cell.symbol);
        //     }
        // }
        let patches = self.curr_buffer.diff(&self.prev_buffer);
        for Patch { cell: Cell { symbol, width, fg, bg, }, x, y, } in patches {
            self.stdout.queue(cursor::MoveTo(x as u16, y as u16))?;
            self.stdout.queue(style::PrintStyledContent(
                symbol
                    .with(fg.unwrap_or(Color::Reset))
                    .on(bg.unwrap_or(Color::Reset)),
            ))?;
        }

        self.prev_buffer = std::mem::replace(
            &mut self.curr_buffer,
            RenderBuffer::with_size(self.width, self.height),
        );

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
                        self.curr_buffer.put_symbol(g.to_string(), width as u8, x, y, None, None);
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

        Self { rope, rows, cols }
    }

    fn byte_offset_at_cursor(&self, cursor_x: usize, cursor_y: usize) -> usize {
        let mut offset = self.rope.byte_of_line(cursor_y);
        for (i, g) in self.rope.line(cursor_y).graphemes().enumerate() {
            if i == cursor_x {
                break;
            }
            offset += g.len();
        }
        offset
    }

    fn insert_char_at(&mut self, char: char, cursor_x: usize, cursor_y: usize) {
        let offset = self.byte_offset_at_cursor(cursor_x, cursor_y);
        let mut buf = [0, 4];
        let text = char.encode_utf8(&mut buf);

        self.cols[cursor_y] += unicode_display_width::width(&text) as usize;
        self.rope.insert(offset, text);
    }
}

enum CursorHorizontalMovementDirection {
    Left,
    Right,
}

enum CursorVerticalMovementDirection {
    Up,
    Down,
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
    sticky_cursor_x: usize,
    document_cursor_x: usize,
    document_cursor_y: usize,
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
            sticky_cursor_x: 0,
            document_cursor_x: 0,
            document_cursor_y: 0,
        }
    }

    fn row_range(&self) -> std::ops::Range<usize> {
        self.scroll_y..self.scroll_y + self.height
    }

    fn col_range(&self) -> std::ops::Range<usize> {
        self.scroll_x..self.scroll_x + self.width
    }

    fn move_document_cursor_to(&mut self, x: Option<usize>, y: Option<usize>, buffer: &TextBuffer) {
        let y = buffer.rows.saturating_sub(1).min(y.unwrap_or(self.document_cursor_y));
        let x = buffer.cols[y].min(x.unwrap_or(self.sticky_cursor_x));


        let horizontal_direction = match self.document_cursor_x.cmp(&x) {
            Ordering::Greater => Some(CursorHorizontalMovementDirection::Left),
            Ordering::Less => Some(CursorHorizontalMovementDirection::Right),
            Ordering::Equal => None,
        };

        let vertical_direction = match self.document_cursor_y.cmp(&y) {
            Ordering::Greater => Some(CursorVerticalMovementDirection::Up),
            Ordering::Less => Some(CursorVerticalMovementDirection::Down),
            Ordering::Equal => None,
        };

        self.document_cursor_x = x;
        self.document_cursor_y = y;

        self.scroll_into_view(buffer, horizontal_direction, vertical_direction);
    }

    fn scroll_into_view(&mut self, buffer: &TextBuffer, horizontal_direction: Option<CursorHorizontalMovementDirection>, vertical_direction: Option<CursorVerticalMovementDirection>) {
        if let Some(dir) = vertical_direction {
            match dir {
                CursorVerticalMovementDirection::Up => self.scroll_up(buffer),
                CursorVerticalMovementDirection::Down => self.scroll_down(buffer),
            }
        }

        if let Some(dir) = horizontal_direction {
            match dir {
                CursorHorizontalMovementDirection::Left => self.scroll_left(),
                CursorHorizontalMovementDirection::Right => self.scroll_right(buffer),
            }
        }
    }

    fn cursor_up(&mut self, buffer: &TextBuffer) {
        self.move_document_cursor_to(
            None,
            Some(self.document_cursor_y.saturating_sub(1)),
            buffer,
        );
    }

    fn cursor_down(&mut self, buffer: &TextBuffer) {
        self.move_document_cursor_to(
            None,
            Some(self.document_cursor_y + 1),
            buffer,
        );
    }

    fn cursor_left(&mut self, rope: &TextBuffer) {
        self.move_document_cursor_to(
            Some(self.document_cursor_x.saturating_sub(1)),
            None,
            rope,
        );

        self.sticky_cursor_x = self.document_cursor_x;
    }

    fn cursor_right(&mut self, rope: &TextBuffer) {
        self.move_document_cursor_to(
            Some(self.document_cursor_x + 1),
            None,
            rope,
        );

        self.sticky_cursor_x = self.document_cursor_x;
    }

    fn scroll_up(&mut self, _buffer: &TextBuffer) {
        let scroll_y = self.document_cursor_y.saturating_sub(self.offset_y).min(self.scroll_y);
        self.scroll_y = scroll_y;

        self.cursor_y = self.document_cursor_y.saturating_sub(self.scroll_y);
    }

    fn scroll_down(&mut self, buffer: &TextBuffer) {
        let max_scroll_y = buffer.rows.saturating_sub(self.height);
        let scroll_y = self.document_cursor_y.saturating_sub(self.height.saturating_sub(self.offset_y + 1)).min(max_scroll_y);
        self.scroll_y = self.scroll_y.max(scroll_y);

        self.cursor_y = self.document_cursor_y.saturating_sub(self.scroll_y);
    }

    fn scroll_left(&mut self) {
        let scroll_x = self.document_cursor_x.saturating_sub(self.offset_x).min(self.scroll_x);
        self.scroll_x = scroll_x;

        self.cursor_x = self.document_cursor_x.saturating_sub(self.scroll_x);
    }

    fn scroll_right(&mut self, buffer: &TextBuffer) {
        let max_scroll_x = buffer.cols.iter().max().unwrap_or(&0).saturating_sub(self.width);
        let scroll_x = self.document_cursor_x.saturating_sub(self.width.saturating_sub(self.offset_x + 1)).min(max_scroll_x);
        self.scroll_x = self.scroll_x.max(scroll_x);

        self.cursor_x = self.document_cursor_x.saturating_sub(self.scroll_x);
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
        // if let Mode::Insert = self.mode {
        //     self.buffer_view.cursor_left();
        // }
        self.mode = Mode::Normal;
    }

    fn enter_insert_mode_relative_to_cursor(&mut self, x: usize) {
        for _ in 0..x {
            self.buffer_view.cursor_right(&self.text_buffer);
        }
        self.mode = Mode::Insert;
    }

    fn append_character(&mut self, c: char) {
        self.text_buffer.insert_char_at(c, self.buffer_view.document_cursor_x, self.buffer_view.document_cursor_y);
        self.buffer_view.cursor_right(&self.text_buffer);
    }

    // fn delete_symbol_to_the_left(&mut self) {
    //     let (cursor_x, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
    //     if cursor_x > 0 {
    //         let offset = byte_offset_at_cursor(&self.rope, cursor_x - 1, cursor_y);


    //         self.buffer_view.cursor_left();
    //     }
    // }

    // fn insert_line_break(&mut self) {
    //     let (cursor_x, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
    //     let right = self.rope.lines[cursor_y].split_off(cursor_x);
    //     self.insert_line_relative_to_cursor(1, right);
    // }

    // fn insert_line_below(&mut self) {
    //     self.insert_line_relative_to_cursor(1, "".to_string());
    // }

    // fn insert_line_above(&mut self) {
    //     self.insert_line_relative_to_cursor(0, "".to_string());
    // }

    // fn insert_line_relative_to_cursor(&mut self, y: usize, contents: String) {
    //     let (_, cursor_y) = self.buffer_view.cursor_position_relative_to_buffer();
    //     self.rope.lines.insert(cursor_y + y, contents);
    //     for _ in 0..y {
    //         self.buffer_view.cursor_down(&self.rope);
    //     }
    //     self.buffer_view.cursor_x = 0;
    //     self.mode = Mode::Insert;
    // }
}

#[derive(PartialEq, Debug, Clone)]
struct Cell {
    symbol: String,
    width: u8,
    fg: Option<Color>,
    bg: Option<Color>,
}

impl Cell {
    fn empty() -> Self {
        Self {
            symbol: " ".to_string(),
            width: 1,
            fg: None,
            bg: None,
        }
    }
}

#[derive(Debug)]
struct Patch {
    cell: Cell,
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

    fn diff(&self, other: &Self) -> Vec<Patch> {
        assert!(self.width == other.width && self.height == other.height);

        self.cells
            .iter()
            .zip(other.cells.iter())
            .enumerate()
            .filter(|(_, (a, b))| *a != *b)
            .map(|(i, (cell, _))| Patch {
                cell: cell.clone(),
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

    fn put_symbol(&mut self, symbol: String, width: u8, x: usize, y: usize, fg: Option<Color>, bg: Option<Color>) {
        let index = self.width * y + x;

        if let Some(cell) = self.cells.get_mut(index) {
            *cell = Cell { symbol, width, fg, bg };
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
                                editor.buffer_view.cursor_up(&editor.text_buffer);
                            }
                            Action::MoveDown => {
                                editor.buffer_view.cursor_down(&editor.text_buffer);
                            }
                            Action::MoveLeft => {
                                editor.buffer_view.cursor_left(&editor.text_buffer);
                            }
                            Action::MoveRight => {
                                editor.buffer_view.cursor_right(&editor.text_buffer);
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
                            Action::NewLine => {
                                // editor.insert_line_break();
                            }
                            Action::InsertLineBelow => {
                                // editor.insert_line_below();
                            }
                            Action::InsertLineAbove => {
                                // editor.insert_line_above();
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
