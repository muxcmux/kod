use std::io::{self, stdout, Write};

use anyhow::Result;
use crossterm::{cursor::{self, SetCursorStyle}, event, queue, style::{Attribute, Color, Colors, Print, SetAttribute, SetBackgroundColor, SetColors, SetForegroundColor, SetUnderlineColor}, terminal::{self, Clear, ClearType}, ExecutableCommand, QueueableCommand};

use super::{buffer::{Buffer, Patch}, style::{Modifier, UnderlineStyle}, Position, Rect};

pub fn enter_terminal_screen() -> Result<()> {
    let mut stdout = std::io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(event::EnableBracketedPaste)?;
    stdout.execute(terminal::EnterAlternateScreen)?;
    stdout.execute(terminal::Clear(terminal::ClearType::All))?;

    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        _ = leave_terminal_screen();
        println!();
        default_panic(info);
    }));

    Ok(())
}

pub fn leave_terminal_screen() -> Result<()> {
    terminal::disable_raw_mode()?;
    stdout().execute(event::DisableBracketedPaste)?;
    stdout().execute(terminal::LeaveAlternateScreen)?;

    Ok(())
}

pub struct Terminal {
    buffers: [Buffer; 2],
    current: usize,
    size: Rect,
}

impl Terminal {
    pub fn new(size: Rect) -> Self {
        let buffers = [
            Buffer::new(size),
            Buffer::new(size)
        ];

        Self {
            buffers,
            current: 0,
            size,
        }
    }

    pub fn current_buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffers[self.current]
    }

    pub fn resize(&mut self, size: Rect) -> Result<()> {
        self.buffers[self.current].resize(size);
        self.buffers[1 - self.current].resize(size);
        self.size = size;
        self.clear()
    }

    pub fn clear(&mut self) -> Result<()> {
        stdout().execute(Clear(ClearType::All))?;
        self.buffers[1 - self.current].reset();

        Ok(())
    }

    pub fn flush(&self) -> Result<()> {
        stdout().flush()?;

        Ok(())
    }

    pub fn draw(&mut self) -> Result<()> {
        let mut stdout = stdout();

        let prev_buffer = &self.buffers[1 - self.current];
        let curr_buffer = &self.buffers[self.current];

        let mut fg = Color::Reset;
        let mut bg = Color::Reset;
        let mut underline_color = Color::Reset;
        let mut underline_style = UnderlineStyle::Reset;
        let mut modifier = Modifier::empty();

        for Patch { cell, x, y, } in prev_buffer.diff(curr_buffer) {
            stdout.queue(cursor::MoveTo(x as u16, y as u16))?;


            if cell.modifier != modifier {
                let diff = ModifierDiff {
                    from: modifier,
                    to: cell.modifier,
                };
                diff.queue(&mut stdout)?;
                modifier = cell.modifier;
            }

            if cell.fg != fg {
                stdout.queue(SetForegroundColor(cell.fg))?;
                fg = cell.fg;
            }

            if cell.bg != bg {
                stdout.queue(SetBackgroundColor(cell.bg))?;
                bg = cell.bg;
            }

            if cell.underline_color != underline_color {
                stdout.queue(SetUnderlineColor(cell.underline_color))?;
                underline_color = cell.underline_color;
            }

            if cell.underline_style != underline_style {
                stdout.queue(SetAttribute(cell.underline_style.into()))?;
                underline_style = cell.underline_style;
            }

            stdout.queue(Print(&cell.symbol))?;
        }

        // reset everything at the end of the frame
        stdout.queue(SetColors(Colors::new(Color::Reset, Color::Reset)))?;
        stdout.queue(SetUnderlineColor(Color::Reset))?;
        stdout.queue(SetAttribute(Attribute::Reset))?;

        // swap the buffers
        self.buffers[1 - self.current].reset();
        self.current = 1 - self.current;

        Ok(())
    }

    pub fn hide_cursor(&self) -> Result<()> {
        let mut stdout = stdout();
        stdout.queue(cursor::Hide)?;
        Ok(())
    }

    pub fn show_cursor(&self) -> Result<()> {
        let mut stdout = stdout();
        stdout.queue(cursor::Show)?;
        Ok(())
    }

    pub fn set_cursor(&self, position: Position, style: SetCursorStyle) -> Result<()> {
        let mut stdout = stdout();
        stdout.queue(cursor::MoveTo(position.col, position.row))?;
        stdout.queue(style)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ModifierDiff {
    pub from: Modifier,
    pub to: Modifier,
}

impl ModifierDiff {
    fn queue<W>(&self, mut w: W) -> io::Result<()>
    where
        W: io::Write,
    {
        let removed = self.from - self.to;
        if removed.contains(Modifier::REVERSED) {
            queue!(w, SetAttribute(Attribute::NoReverse))?;
        }
        if removed.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(Attribute::NormalIntensity))?;
            if self.to.contains(Modifier::DIM) {
                queue!(w, SetAttribute(Attribute::Dim))?;
            }
        }
        if removed.contains(Modifier::ITALIC) {
            queue!(w, SetAttribute(Attribute::NoItalic))?;
        }
        if removed.contains(Modifier::DIM) {
            queue!(w, SetAttribute(Attribute::NormalIntensity))?;
        }
        if removed.contains(Modifier::CROSSED_OUT) {
            queue!(w, SetAttribute(Attribute::NotCrossedOut))?;
        }
        if removed.contains(Modifier::SLOW_BLINK) || removed.contains(Modifier::RAPID_BLINK) {
            queue!(w, SetAttribute(Attribute::NoBlink))?;
        }
        if removed.contains(Modifier::HIDDEN) {
            queue!(w, SetAttribute(Attribute::NoHidden))?;
        }

        let added = self.to - self.from;
        if added.contains(Modifier::REVERSED) {
            queue!(w, SetAttribute(Attribute::Reverse))?;
        }
        if added.contains(Modifier::BOLD) {
            queue!(w, SetAttribute(Attribute::Bold))?;
        }
        if added.contains(Modifier::ITALIC) {
            queue!(w, SetAttribute(Attribute::Italic))?;
        }
        if added.contains(Modifier::DIM) {
            queue!(w, SetAttribute(Attribute::Dim))?;
        }
        if added.contains(Modifier::CROSSED_OUT) {
            queue!(w, SetAttribute(Attribute::CrossedOut))?;
        }
        if added.contains(Modifier::SLOW_BLINK) {
            queue!(w, SetAttribute(Attribute::SlowBlink))?;
        }
        if added.contains(Modifier::RAPID_BLINK) {
            queue!(w, SetAttribute(Attribute::RapidBlink))?;
        }
        if added.contains(Modifier::HIDDEN) {
            queue!(w, SetAttribute(Attribute::Hidden))?;
        }

        Ok(())
    }
}
