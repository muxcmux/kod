use anyhow::Result;
use crop::Rope;
use crossterm::{
    cursor,
    event::{read, Event, KeyCode, KeyEvent, KeyEventKind},
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal, ExecutableCommand, QueueableCommand,
};
use log::debug;
use unicode_segmentation::UnicodeSegmentation;
use std::{cmp::Ordering, env, io::Write, path::PathBuf};

struct Perf<'a> {
    now: std::time::Instant,
    msg: &'a str
}

impl<'a> Perf<'a> {
    fn new(msg: &'a str) -> Self {
        Self {
            msg,
            now: std::time::Instant::now(),
        }
    }

    fn end(self) {
        debug!("[{}ms] {}", self.now.elapsed().as_millis(), self.msg);
    }
}

macro_rules! perf {
    ($s:expr) => {
        Perf::new($s)
    }
}

enum Mode {
    Normal,
    Insert,
}

enum Action {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    GoToFirstLine,
    GoToLastLine,
    EnterNormalMode,
    EnterInsertModeAtCursor,
    EnterInsertModeAfterCursor,
    EnterInsertModeAtEol,
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
        KeyCode::Char('A') => Some(Action::EnterInsertModeAtEol),
        KeyCode::Char('o') => Some(Action::InsertLineBelow),
        KeyCode::Char('O') => Some(Action::InsertLineAbove),
        KeyCode::Char('g') => Some(Action::GoToFirstLine),
        KeyCode::Char('G') => Some(Action::GoToLastLine),
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
        debug!("Terminal size: {}x{}", width, height);
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
        self.draw_statusline(state);
        self.paint()?;
        self.draw_cursor(state)?;
        self.stdout.flush()?;
        Ok(())
    }

    fn paint(&mut self) -> Result<()> {
        let prev_buffer = &self.buffers[1 - self.current];
        let curr_buffer = &self.buffers[self.current];

        let mut fg = Color::Reset;
        let mut bg = Color::Reset;

        for Patch { cell, x, y, } in prev_buffer.diff(curr_buffer) {
            self.stdout.queue(cursor::MoveTo(x as u16, y as u16))?;

            if cell.fg != fg {
                self.stdout.queue(SetForegroundColor(cell.fg))?;
                fg = cell.fg;
            }
            if cell.bg != bg {
                self.stdout.queue(SetBackgroundColor(cell.bg))?;
                bg = cell.bg;
            }

            self.stdout.queue(Print(&cell.symbol))?;
        }

        self.stdout.queue(SetForegroundColor(Color::Reset))?;
        self.stdout.queue(SetBackgroundColor(Color::Reset))?;

        self.buffers[1 - self.current].reset();
        self.current = 1 - self.current;

        Ok(())
    }

    fn draw_buffer(&mut self, state: &Editor) {
        for row in state.buffer_view.row_range() {
            if row >= state.text_buffer.lines_len() {
                break;
            }
            let line = state.text_buffer.data.line(row);
            let mut graphemes = line.graphemes();
            let mut skip_next_n_cols = 0;

            // advance the iterator to account for scroll
            let mut advance = 0;
            while advance < state.buffer_view.scroll_x {
                if let Some(g) = graphemes.next() {
                    advance += unicode_display_width::width(&g) as usize;
                    skip_next_n_cols = advance.saturating_sub(state.buffer_view.scroll_x);
                } else {
                    break
                }
            }

            for col in state.buffer_view.col_range() {
                if skip_next_n_cols > 0 {
                    skip_next_n_cols -= 1;
                    continue;
                }
                match graphemes.next() {
                    None => break,
                    Some(g) => {
                        let width = unicode_display_width::width(&g) as usize;
                        let x = col.saturating_sub(state.buffer_view.scroll_x);
                        let y = row.saturating_sub(state.buffer_view.scroll_y);
                        self.current_buffer_mut().put_symbol(g.to_string(), x, y, Color::Reset, Color::Reset);
                        skip_next_n_cols = width - 1;
                    }
                }
            }
        }
    }

    fn draw_statusline(&mut self, state: &Editor) {
        let h = self.height - 1;
        let line = " ".repeat(self.width);
        self.current_buffer_mut().put_string(line, 0, h, Color::White, Color::Black);

        let (label, label_fg, label_bg) = match state.mode {
            Mode::Normal => (" NOR ", Color::Black, Color::Blue),
            Mode::Insert => (" INS ", Color::Black, Color::Green),
        };

        self.current_buffer_mut().put_string(label.to_string(), 0, h, label_fg, label_bg);

        let filename = match &state.text_buffer.path {
            Some(p) => p.to_str().expect("shit path name given"),
            None => "[scratch]",
        };
        self.current_buffer_mut().put_string(filename.to_string(), label.chars().count() + 1, h, Color::White, Color::Black);

        if state.text_buffer.modified {
            let x = filename.chars().count() + label.chars().count() + 2;
            self.current_buffer_mut().put_string("[*]".to_string(), x, h, Color::White, Color::Black);
        }

        let cursor_position = format!(" {}:{} ", state.text_buffer.cursor_y + 1, state.text_buffer.grapheme_idx_at_cursor() + 1);
        let w = self.width - cursor_position.chars().count();
        self.current_buffer_mut().put_string(cursor_position, w, h, Color::White, Color::Black);
    }

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
    data: Rope,
    cursor_x: usize,
    cursor_y: usize,
    sticky_cursor_x: usize,
    path: Option<PathBuf>,
    modified: bool,
    last_vertical_move_dir: Option<VerticalMove>,
    last_horizontal_move_dir: Option<HorizontalMove>,
}

impl TextBuffer {
    fn new(data: Rope, path: Option<PathBuf>) -> Self {
        Self {
            data,
            path,
            cursor_x: 0,
            cursor_y: 0,
            sticky_cursor_x: 0,
            last_vertical_move_dir: None,
            last_horizontal_move_dir: None,
            modified: false,
        }
    }

    fn byte_offset_at_cursor(&self, cursor_x: usize, cursor_y: usize) -> usize {
        let mut offset = self.data.byte_of_line(cursor_y);
        let mut col = 0;
        for g in self.data.line(cursor_y).graphemes() {
            if col == cursor_x {
                break;
            }
            col += unicode_display_width::width(&g) as usize;
            offset += g.len();
        }
        offset
    }

    fn insert_char_at_cursor(&mut self, char: char, mode: &Mode) {
        self.modified = true;
        let offset = self.byte_offset_at_cursor(self.cursor_x, self.cursor_y);
        let mut buf = [0; 4];
        let text = char.encode_utf8(&mut buf);

        self.data.insert(offset, text);

        if char == '\n' {
            self.move_cursor_to(Some(0), Some(self.cursor_y + 1), mode);
        } else {
            self.move_cursor_to(Some(self.cursor_x + 1), None, mode);
        }
    }

    fn grapheme_idx_at_cursor(&self) -> usize {
        let mut idx = 0;
        let mut col = 0;

        let mut iter = self.data.line(self.cursor_y).graphemes().enumerate().peekable();
        while let Some((i, g)) = iter.next() {
            idx = i;
            if col >= self.cursor_x { break }
            if iter.peek().is_none() { idx += 1 }
            col += unicode_display_width::width(&g) as usize;
        }

        idx
    }

    fn delete_to_the_left(&mut self, mode: &Mode) {
        assert!(matches!(mode, Mode::Insert));

        self.modified = true;
        if self.cursor_x > 0 {
            let mut start = self.data.byte_of_line(self.cursor_y);
            let mut end = start;
            let idx = self.grapheme_idx_at_cursor() - 1;
            for (i, g) in self.data.line(self.cursor_y).graphemes().enumerate() {
                if i < idx { start += g.len() }
                if i == idx {
                    end = start + g.len();
                    break
                }
            }
            self.data.delete(start..end);
            self.cursor_left(&Mode::Insert);
        } else if self.cursor_y > 0 {
            let byte_length_of_newline_char = 1;
            let to = self.data.byte_of_line(self.cursor_y);
            let from = to.saturating_sub(byte_length_of_newline_char);
            // need to move cursor before deleting
            self.move_cursor_to(Some(self.line_len(self.cursor_y - 1)), Some(self.cursor_y - 1), mode);
            self.data.delete(from..to);
        }
    }

    fn lines_len(&self) -> usize {
        self.data.lines().len()
    }

    fn line_len(&self, line: usize) -> usize {
        self.data.line(line).graphemes().map(|g| unicode_display_width::width(&g) as usize).sum()
    }

    fn current_line_len(&self) -> usize {
        self.line_len(self.cursor_y)
    }

    fn move_cursor_to(&mut self, x: Option<usize>, y: Option<usize>, mode: &Mode) {
        // ensure x and y are within bounds
        let y = self.lines_len().saturating_sub(1).min(y.unwrap_or(self.cursor_y));
        let max_x = match mode {
            Mode::Insert => self.line_len(y),
            Mode::Normal => self.line_len(y).saturating_sub(1),
        };
        let x = max_x.min(x.unwrap_or(self.sticky_cursor_x));

        self.last_horizontal_move_dir = match self.cursor_x.cmp(&x) {
            Ordering::Greater => Some(HorizontalMove::Left),
            Ordering::Less => Some(HorizontalMove::Right),
            Ordering::Equal => None,
        };

        self.last_vertical_move_dir = match self.cursor_y.cmp(&y) {
            Ordering::Greater => Some(VerticalMove::Up),
            Ordering::Less => Some(VerticalMove::Down),
            Ordering::Equal => None,
        };

        self.cursor_x = x;
        self.cursor_y = y;

        self.ensure_cursor_is_on_grapheme_boundary(mode);
    }

    fn ensure_cursor_is_on_grapheme_boundary(&mut self, mode: &Mode) {
        let mut acc = 0;
        let go_to_prev = self.last_vertical_move_dir.is_some() || matches!(self.last_horizontal_move_dir, Some(HorizontalMove::Left));
        let go_to_next = matches!(self.last_horizontal_move_dir, Some(HorizontalMove::Right));

        let mut graphemes = self.data.line(self.cursor_y).graphemes().peekable();

        while let Some(g) = graphemes.next() {
            let width = unicode_display_width::width(&g) as usize;

            let next_grapheme_start = acc + width;

            if (self.cursor_x < next_grapheme_start) && (self.cursor_x > acc) {
                if go_to_prev {
                    self.cursor_x = acc;
                } else if go_to_next {
                    if graphemes.peek().is_none() && !matches!(mode, Mode::Insert) {
                        self.cursor_x = acc;
                    } else {
                        self.cursor_x = next_grapheme_start;
                    }
                }
                break;
            }

            acc += width;
        }
    }

    fn cursor_up(&mut self, mode: &Mode) {
        self.move_cursor_to(None, Some(self.cursor_y.saturating_sub(1)), mode);
    }

    fn cursor_down(&mut self, mode: &Mode) {
        self.move_cursor_to(None, Some(self.cursor_y + 1), mode);
    }

    fn cursor_left(&mut self, mode: &Mode) {
        self.move_cursor_to(Some(self.cursor_x.saturating_sub(1)), None, mode);

        self.sticky_cursor_x = self.cursor_x;
    }

    fn cursor_right(&mut self, mode: &Mode) {
        self.move_cursor_to(Some(self.cursor_x + 1), None, mode);

        self.sticky_cursor_x = self.cursor_x;
    }
}

enum HorizontalMove { Right, Left }
enum VerticalMove { Down, Up }

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
            offset_x: 10,
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

    fn ensure_cursor_is_in_view(&mut self, buffer: &TextBuffer) {
        if let Some(ref dir) = buffer.last_vertical_move_dir {
            match dir {
                VerticalMove::Up => self.scroll_up(buffer),
                VerticalMove::Down => self.scroll_down(buffer),
            }
        }

        if let Some(ref dir) = buffer.last_horizontal_move_dir {
            match dir {
                HorizontalMove::Left => self.scroll_left(buffer),
                HorizontalMove::Right => self.scroll_right(buffer),
            }
        }

        // adjust cursor
        self.cursor_y = buffer.cursor_y.saturating_sub(self.scroll_y);
        self.cursor_x = buffer.cursor_x.saturating_sub(self.scroll_x);
    }

    fn scroll_up(&mut self, buffer: &TextBuffer) {
        self.scroll_y = buffer.cursor_y.saturating_sub(self.offset_y).min(self.scroll_y);
    }

    fn scroll_down(&mut self, buffer: &TextBuffer) {
        let max_scroll_y = buffer.lines_len().saturating_sub(self.height);
        let scroll_y = buffer.cursor_y.saturating_sub(self.height.saturating_sub(self.offset_y + 1)).min(max_scroll_y);
        self.scroll_y = self.scroll_y.max(scroll_y);
    }

    fn scroll_left(&mut self, buffer: &TextBuffer) {
        let scroll_x = buffer.cursor_x.saturating_sub(self.offset_x).min(self.scroll_x);
        self.scroll_x = scroll_x;
    }

    fn scroll_right(&mut self, buffer: &TextBuffer) {
        let max_scroll_x = buffer.line_len(buffer.cursor_y).saturating_sub(self.width);
        let scroll_x = buffer.cursor_x.saturating_sub(self.width.saturating_sub(self.offset_x + 1)).min(max_scroll_x);
        self.scroll_x = self.scroll_x.max(scroll_x);
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

        let mut path = None;
        let data = if args.len() > 1 {
            let p = perf!("Reading file and building rope");

            let pa = PathBuf::from(args.pop().unwrap());
            let contents = std::fs::read_to_string(&pa)?;
            path = Some(pa);
            let data = Rope::from(contents);

            p.end();

            data
        } else {
            Rope::from("\n")
        };

        let text_buffer = TextBuffer::new(data, path);

        Ok(Self {
            text_buffer,
            mode: Mode::Normal,
            buffer_view: ScrollView::with_size(width.saturating_sub(1), height),
        })
    }

    fn enter_normal_mode(&mut self) {
        self.mode = Mode::Normal;
        self.text_buffer.cursor_left(&self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn enter_insert_mode_relative_to_cursor(&mut self, x: usize) {
        self.mode = Mode::Insert;
        for _ in 0..x {
            self.text_buffer.cursor_right(&self.mode);
            self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
        }
    }

    fn enter_insert_mode_at_eol(&mut self) {
        self.mode = Mode::Insert;
        self.text_buffer.move_cursor_to(Some(self.text_buffer.current_line_len()), None, &self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn append_character(&mut self, c: char) {
        self.text_buffer.insert_char_at_cursor(c, &self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn cursor_up(&mut self) {
        self.text_buffer.cursor_up(&self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn cursor_down(&mut self) {
        self.text_buffer.cursor_down(&self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn cursor_left(&mut self) {
        self.text_buffer.cursor_left(&self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn cursor_right(&mut self) {
        self.text_buffer.cursor_right(&self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn go_to_first_line(&mut self) {
        self.text_buffer.move_cursor_to(None, Some(0), &self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn go_to_last_line(&mut self) {
        self.text_buffer.move_cursor_to(None, Some(self.text_buffer.lines_len() - 1), &self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn insert_line_below(&mut self) {
        self.mode = Mode::Insert;
        self.text_buffer.move_cursor_to(Some(std::usize::MAX), None, &self.mode);
        self.text_buffer.insert_char_at_cursor('\n', &self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn insert_line_above(&mut self) {
        self.mode = Mode::Insert;
        self.text_buffer.move_cursor_to(Some(std::usize::MAX), Some(self.text_buffer.cursor_y.saturating_sub(1)), &self.mode);
        self.text_buffer.insert_char_at_cursor('\n', &self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }

    fn delete_symbol_to_the_left(&mut self) {
        self.text_buffer.delete_to_the_left(&self.mode);
        self.buffer_view.ensure_cursor_is_in_view(&self.text_buffer);
    }
}

#[derive(PartialEq, Debug, Clone)]
struct Cell {
    symbol: String,
    fg: Color,
    bg: Color,
}

impl Cell {
    fn empty() -> Self {
        Self {
            symbol: " ".to_string(),
            fg: Color::Reset,
            bg: Color::Reset,
        }
    }

    fn set_symbol(&mut self, symbol: &str) -> &mut Self {
        self.symbol.clear();
        self.symbol.push_str(symbol);
        self
    }

    fn set_fg(&mut self, fg: Color) -> &mut Self {
        self.fg = fg;
        self
    }

    fn set_bg(&mut self, bg: Color) -> &mut Self {
        self.bg = bg;
        self
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

        let mut patches = vec![];

        let mut invalidated = 0;
        let mut to_skip = 0;
        for (i, (current, previous)) in other.cells.iter().zip(self.cells.iter()).enumerate() {
            if (current != previous || invalidated > 0) && to_skip == 0 {
                let x = i % self.width;
                let y = i / self.width;
                patches.push(Patch { x, y, cell: &other.cells[i] });
            }

            let current_width = unicode_display_width::width(&current.symbol);
            to_skip = current_width.saturating_sub(1);

            let affected_width = std::cmp::max(current_width, unicode_display_width::width(&previous.symbol));
            invalidated = std::cmp::max(affected_width, invalidated).saturating_sub(1);
        }

        patches
    }

    // fn resize(&mut self, width: usize, height: usize) {
    //     self.width = width;
    //     self.height = height;
    //     self.cells.resize(width * height, Cell::empty());
    // }

    fn put_symbol(&mut self, symbol: String, x: usize, y: usize, fg: Color, bg: Color) {
        let index = self.width * y + x;

        if let Some(cell) = self.cells.get_mut(index) {
            cell.set_symbol(&symbol)
                .set_fg(fg)
                .set_bg(bg);
        }
    }

    fn put_string(&mut self, string: String, x: usize, y: usize, fg: Color, bg: Color) {
        let start = self.width * y + x;

        for (offset, g) in string.graphemes(true).enumerate() {
            if start + offset > self.cells.len() {
                break;
            }
            if let Some(cell) = self.cells.get_mut(start + offset) {
                cell.set_symbol(g)
                    .set_fg(fg)
                    .set_bg(bg);
            }
        }
    }
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
                            Action::GoToFirstLine => {
                                editor.go_to_first_line();
                            }
                            Action::GoToLastLine => {
                                editor.go_to_last_line();
                            }
                            Action::EnterInsertModeAtCursor => {
                                editor.enter_insert_mode_relative_to_cursor(0);
                            }
                            Action::EnterInsertModeAfterCursor => {
                                editor.enter_insert_mode_relative_to_cursor(1);
                            }
                            Action::EnterInsertModeAtEol => {
                                editor.enter_insert_mode_at_eol();
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
                                editor.delete_symbol_to_the_left();
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
