use std::path::{Path, PathBuf};
use std::fs::read_dir;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::cmp::Ordering;
use anyhow::{anyhow, bail, Result};

use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent, KeyModifiers}};
use nanoid::nanoid;
use unicode_segmentation::UnicodeSegmentation;

use crate::{graphemes, language::LANG_CONFIG, panes::Layout};
use crate::ui::{Position, Rect};
use crate::ui::theme::THEME;
use crate::ui::text_input::TextInput;
use crate::ui::style::Style;
use crate::ui::scroll::Scroll;
use crate::ui::modal::{YesNoCancel, Modal};
use crate::ui::buffer::Buffer;
use crate::ui::borders::Borders;
use crate::ui::border_box::BorderBox;
use crate::document::cwd_relative_name;
use crate::current;
use crate::compositor::{Component, Context, EventResult};

const ACTIVE_COLUMN_WIDTH: u16 = 52;
const INACTIVE_COLUMN_WIDTH: u16 = 17;

fn sorted_entries(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = read_dir(dir)?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()?;

    entries.sort_by(|a, b| {
        match (a.is_dir(), b.is_dir()) {
            (true, true) | (false, false) => a.cmp(b),
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
        }
    });

    Ok(entries)
}

fn icon(path: &Path) -> (String, Style) {
    if path.is_dir() {
        ("󰉋".into(), THEME.get("ui.files.icon.folder"))
    } else if let Some(config) = LANG_CONFIG.language_config_for_path(path) {
        if let Some(icon) = &config.icon {
            let style = if let Some(c) = &config.color {
                Style::default().fg(*c)
            } else {
                THEME.get("ui.files.icon.file")
            };
            (icon.clone(), style)
        } else {
            ("󰈔".into(), THEME.get("ui.files.icon.file"))
        }
    } else {
        ("󰈔".into(), THEME.get("ui.files.icon.file"))
    }
}

fn delete_path(path: &Path) -> Result<()> {
    if path.metadata()?.is_dir() {
        std::fs::remove_dir_all(path)?;
    } else {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

enum Selection {
    File(PathBuf),
    Dir,
    Invalid,
}

struct Column {
    index: usize,
    scroll: Scroll,
    calculated_area: Rect,
    path: PathBuf,
    paths: Vec<PathBuf>,
}

impl Column {
    fn new(path: PathBuf, selected_file: Option<&PathBuf>) -> Result<Self> {
        let paths = sorted_entries(&path)?;

        let index = selected_file.and_then(|f| paths.iter().position(|i| i == f)).unwrap_or(0);

        Ok(Self {
            path,
            paths,
            index,
            calculated_area: Rect::default(),
            scroll: Scroll::default(),
        })
    }

    fn render(
        &mut self,
        mut area: Rect,
        buffer: &mut Buffer,
        short_title: bool,
        each_row: impl Fn(u16, &Path, Rect, Style, &mut Buffer)
    ) -> Rect {
        let title = if short_title {
            self.path.file_name().unwrap().to_string_lossy()
        } else {
            self.path.to_string_lossy()
        };

        if area.height > 3 {
            area = area.clip_bottom(
                area.height.saturating_sub(self.paths.len().max(1) as u16 + 2)
            );
        }

        let bbox = BorderBox::new(area)
            .title(&title)
            .borders(Borders::ALL)
            .style(THEME.get("ui.border.files"))
            .title_style(THEME.get("ui.files.title"));

        bbox.render(buffer);

        let inner = bbox.inner();

        self.calculated_area = inner;

        self.scroll.adjust_offset(&inner, 0, 3);
        self.scroll.ensure_point_is_visible(0, self.index, &inner, Some(self.paths.len()));

        for i in self.scroll.y..self.scroll.y + inner.height as usize {
            if let Some(path) = self.paths.get(i) {
                let name = path.file_name().expect("No file name for path");
                let name = name.to_string_lossy();

                let name_style = if path.is_dir() {
                    THEME.get("ui.files.folder")
                } else {
                    THEME.get("ui.files.file")
                };

                let y = i.saturating_sub(self.scroll.y) as u16 + inner.top();
                let(icon, icon_style) = icon(path);
                buffer.put_truncated_str(&icon, inner.left(), y, inner.right(), icon_style);
                buffer.put_truncated_str(&name, inner.left() + 2, y, inner.right(), name_style);

                each_row(y, path, inner, name_style, buffer);
            }
        }

        inner
    }
}

#[derive(PartialEq, Eq)]
enum PasteAction {
    Copy,
    Move
}

impl PasteAction {
    fn style(&self) -> Style {
        match self {
            Self::Copy => THEME.get("ui.files.paste.copy"),
            Self::Move => THEME.get("ui.files.paste.move"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum State {
    Browsing,
    Searching,
    Adding,
    Renaming(PathBuf),
    ConfirmDelete(Vec<PathBuf>),
    ConfirmOverwrite(PathBuf),
}

pub struct Files {
    active_column: usize,
    columns: VecDeque<Column>,
    position_cache: HashMap<PathBuf, PathBuf>,
    marked_paths: BTreeSet<PathBuf>,
    yanked_paths: BTreeSet<PathBuf>,
    paste_action: PasteAction,
    modal: Modal,
    state: State,
    search_input: TextInput,
    file_name_input: TextInput,
}

enum StartRenamingCursorPosition {
    Start,
    End,
    FilenameEnd,
    FilenameRemoved,
    NewName,
}

impl Files {
    pub fn new(path: Option<&PathBuf>) -> Result<Self> {
        let (dir, file) = match path {
            Some(p) => {
                if !p.exists() {
                    let parent = p.parent()
                        .map(|d| d.to_path_buf())
                        .unwrap_or(std::env::current_dir()?);
                    (parent, None)
                } else if p.metadata()?.is_dir() {
                    (p.to_path_buf(), None)
                } else if p.metadata()?.is_file() {
                    (p.parent().ok_or(anyhow!("Can't find parent dir of {:?}", p))?.to_path_buf(), Some(p))
                } else {
                    bail!("Given path is neither a file nor a dir")
                }
            }
            None => (std::env::current_dir()?, None),
        };

        let columns = VecDeque::from([Column::new(dir.clone(), file)?]);

        let mut position_cache = HashMap::new();
        if let Some(f) = file {
            position_cache.insert(dir, f.to_path_buf());
        }

        Ok(Self {
            position_cache,
            columns,
            active_column: 0,
            marked_paths: BTreeSet::new(),
            yanked_paths: BTreeSet::new(),
            paste_action: PasteAction::Copy,
            state: State::Browsing,
            modal: Modal::new("⚠ Confirm".into(), "".into()),
            search_input: TextInput::empty(),
            file_name_input: TextInput::empty(),
        })
    }

    fn close_children(&mut self) {
        let total = self.columns.len();
        for _ in self.active_column..total.saturating_sub(1) {
            self.columns.pop_back();
        }
        // update position cache
        let col = &self.columns[self.active_column];

        if let Some(p) = col.paths.get(col.index) {
            self.position_cache.insert(col.path.clone(), p.clone());
        }
    }

    fn goto_path(&mut self, path: &Path) {
        for (i, p) in self.columns[self.active_column].paths.iter().enumerate() {
            if path.ancestors().any(|a| a == p) {
                self.columns[self.active_column].index = i;
                break;
            }
        }
    }

    fn move_up(&mut self) {
        let col = &mut self.columns[self.active_column];

        if col.index > 0 {
            col.index -= 1;
            self.close_children();
        }
    }

    fn move_down(&mut self) {
        let col = &mut self.columns[self.active_column];

        if col.index < col.paths.len().saturating_sub(1) {
            col.index += 1;
            self.close_children();
        }
    }

    fn move_half_page_up(&mut self) {
        let col = &mut self.columns[self.active_column];

        if col.index > 0 {
            col.index = col.index.saturating_sub((col.calculated_area.height / 2).into());
            self.close_children();
        }
    }

    fn move_half_page_down(&mut self) {
        let col = &mut self.columns[self.active_column];

        let len = col.paths.len().saturating_sub(1);
        if col.index < len {
            let plus_half = col.index + (col.calculated_area.height / 2) as usize;
            col.index = plus_half.min(len);
            self.close_children();
        }
    }

    fn move_top(&mut self) {
        let col = &mut self.columns[self.active_column];

        if col.index > 0 {
            col.index = 0;
            self.close_children();
        }
    }

    fn move_bottom(&mut self) {
        let col = &mut self.columns[self.active_column];

        if col.index < col.paths.len().saturating_sub(1) {
            col.index = col.paths.len().saturating_sub(1);
            self.close_children();
        }
    }

    fn move_to_first_search_match(&mut self) {
        let col = &mut self.columns[self.active_column];

        for (i, path) in col.paths.iter().enumerate() {
            if let Some(path) = path.file_name().and_then(|p| p.to_str()) {
                if path.to_lowercase().contains(&self.search_input.value().to_lowercase()) {
                    col.index = i;
                    break;
                }
            }
        }
    }

    fn parent(&mut self) -> Result<()> {
        if self.active_column > 0 {
            self.active_column -= 1;
            self.marked_paths.clear();
        } else {
            let path = &self.columns[self.active_column].path;
            if let Some(parent) = path.parent() {
                let col = Column::new(parent.to_path_buf(), Some(path))?;
                self.position_cache.insert(parent.to_path_buf(), path.clone());
                self.columns.push_front(col);
                self.marked_paths.clear();
            }
        }

        Ok(())
    }

    fn select(&mut self) -> Result<Selection> {
        let col = &self.columns[self.active_column];

        if let Some(marked) = col.paths.get(col.index) {
            if marked.metadata()?.is_dir() {
                // Prevent navigating into directories marked for yanking
                // in order to avoid a whole can of worms that comes with
                // modifying nested paths
                if self.yanked_paths.contains(marked) {
                    return Ok(Selection::Invalid);
                }
                // If the dir is not open on the right, open it
                if self.columns.get(self.active_column + 1).is_none() {
                    let selected = self.position_cache.get(marked);
                    self.columns.push_back(Column::new(marked.to_path_buf(), selected)?);
                }
                // Finally set the active column to the newly opened one
                // and clear the marked paths (not the yanked_paths!)
                self.active_column += 1;
                self.marked_paths.clear();
                return Ok(Selection::Dir)
            } else if marked.metadata()?.is_file() {
                return Ok(Selection::File(marked.to_path_buf()))
            }
        }

        Ok(Selection::Invalid)
    }

    fn open(&mut self, ctx: &mut Context, split: Option<Layout>, close_files: bool) -> Result<EventResult> {
        if let Selection::File(path) = self.select()? {
            let (pane, _) = current!(ctx.editor);
            let pane_id = pane.id;
            match ctx.editor.open(pane_id, &path, split)? {
                Some(callback) => {
                    return Ok(EventResult::Consumed(Some(Box::new(move |compositor, cx| {
                        if close_files {
                            compositor.pop();
                        }
                        callback(compositor, cx);
                    }))));
                }
                None => {
                    if close_files {
                        return Ok(self.dismiss());
                    }
                }
            }
        }

        Ok(EventResult::Consumed(None))
    }

    fn mark(&mut self) -> EventResult {
        self.yanked_paths.clear();

        let col = &self.columns[self.active_column];
        if let Some(marked) = col.paths.get(col.index) {
            if self.marked_paths.contains(marked) {
                self.marked_paths.remove(marked);
            } else {
                self.marked_paths.insert(marked.to_path_buf());
            }

            self.move_down();
        }

        EventResult::Consumed(None)
    }

    fn copy(&mut self) -> Option<usize> {
        if self.marked_paths.is_empty() {
            let col = &self.columns[self.active_column];
            if let Some(marked) = col.paths.get(col.index) {
                self.marked_paths.clear();
                if self.yanked_paths.contains(marked) {
                    self.yanked_paths.clear();
                    return None
                }
                self.yanked_paths.clear();
                self.yanked_paths.insert(marked.to_path_buf());
                return Some(1)
            }
        } else {
            std::mem::swap(&mut self.marked_paths, &mut self.yanked_paths);
            return Some(self.yanked_paths.len())
        }

        None
    }

    fn yank(&mut self, ctx: &mut Context) -> EventResult {
        self.paste_action = PasteAction::Copy;
        if let Some(count) = self.copy() {
            ctx.editor.set_status(format!("Yanked {count} paths(s)"));
        }

        EventResult::Consumed(None)
    }

    fn cut(&mut self, ctx: &mut Context) -> EventResult {
        self.paste_action = PasteAction::Move;
        if let Some(count) = self.copy() {
            ctx.editor.set_status(format!("Cut {count} paths(s)"));
        }

        EventResult::Consumed(None)
    }

    fn try_paste(&mut self) -> Result<EventResult> {
        self.reset()?;

        if self.yanked_paths.is_empty() {
            return Ok(EventResult::Consumed(None))
        }

        let dest_dir = &self.columns[self.active_column].path;
        let mut last = PathBuf::new();
        // This relies on the fact that self.select disallows
        // navigating into directories marked for yanking
        while let Some(mut path) = self.yanked_paths.pop_first() {
            if let Some(parent) = path.parent() {
                if parent == dest_dir {
                    path = next_available_path_name(&path);
                }
            }

            if let Some(file_or_dir) = path.file_name().and_then(|f| f.to_str()) {
                let new_path = dest_dir.join(file_or_dir);
                if new_path.exists() {
                    self.state = State::ConfirmOverwrite(path);
                    return Ok(EventResult::Consumed(None))
                }
                last = new_path
            }

            // These ops are blocking and are running in the main
            // thread so they can block the ui for larger files
            // or large amount of yanked paths
            if let Err(e) = self.paste(&path, dest_dir) {
                self.reset()?;
                return Err(e);
            }
        }

        self.close_children();
        self.reset()?;
        self.goto_path(&last);

        Ok(EventResult::Consumed(None))
    }

    fn paste(&self, path: &Path, dest_dir: &Path) -> Result<()> {
        match self.paste_action {
            PasteAction::Copy => copy_path_to_dir(path, dest_dir)?,
            PasteAction::Move => move_path_to_dir(path, dest_dir)?,
        }

        Ok(())
    }

    fn try_delete(&mut self) -> Result<EventResult> {
        let mut confirm_paths = vec![];
        if self.marked_paths.is_empty() {
            let col = &self.columns[self.active_column];
            if let Some(marked) = col.paths.get(col.index) {
                confirm_paths.push(marked.to_path_buf());
            }
        } else {
            while let Some(path) = self.marked_paths.pop_first() {
                confirm_paths.push(path);
            }
        }

        if !confirm_paths.is_empty() {
            self.state = State::ConfirmDelete(confirm_paths)
        }

        Ok(EventResult::Consumed(None))
    }

    fn start_rename(&mut self, pos: StartRenamingCursorPosition) -> EventResult {
        if let Some(col) = self.columns.get(self.active_column) {
            if let Some(path) = col.paths.get(col.index) {
                if let Some(name) = path.file_name().and_then(|p| p.to_str()) {
                    match pos {
                        StartRenamingCursorPosition::Start => {
                            self.file_name_input.set_value(name);
                            self.file_name_input.move_cursor_to(0);
                        },
                        StartRenamingCursorPosition::End => {
                            self.file_name_input.set_value(name);
                            self.file_name_input.move_cursor_to(usize::MAX);
                        },
                        StartRenamingCursorPosition::FilenameEnd => {
                            self.file_name_input.set_value(name);
                            let mut col = name.graphemes(true).count();
                            let mut has_dot = false;
                            for g in name.graphemes(true).rev() {
                                col = col.saturating_sub(1);
                                if g == "." {
                                    has_dot = true;
                                    break
                                }
                            }
                            if has_dot {
                                self.file_name_input.move_cursor_to(col);
                            } else {
                                self.file_name_input.move_cursor_to(usize::MAX);
                            }
                        },
                        StartRenamingCursorPosition::FilenameRemoved => {
                            let last = name.split('.').last().unwrap();
                            if last == name {
                                self.file_name_input.clear();
                            } else {
                                self.file_name_input.set_value(&format!(".{}", last));
                            }
                            self.file_name_input.move_cursor_to(0);
                        },
                        StartRenamingCursorPosition::NewName => {
                            self.file_name_input.clear();
                            self.file_name_input.move_cursor_to(0);
                        },
                    }

                    self.state = State::Renaming(path.clone())
                }
            }
        }

        EventResult::Consumed(None)
    }

    fn start_add(&mut self) -> EventResult {
        self.file_name_input.clear();
        self.state = State::Adding;
        let col = &mut self.columns[self.active_column];
        col.paths.insert((col.index + 1).min(col.paths.len()), col.path.clone());
        self.move_down();

        EventResult::Consumed(None)
    }

    fn rename(&mut self, ctx: &mut Context) -> Result<EventResult> {
        let col = &self.columns[self.active_column];

        if let Some(current_path) = col.paths.get(col.index) {
            let value = self.file_name_input.value();
            let new_path = col.path.join(&value);
            rename_is_valid(current_path, &new_path, &value)?;

            if current_path == &new_path {
                self.state = State::Browsing;
                return Ok(EventResult::Consumed(None))
            }

            if new_path.starts_with(current_path) {
                // if renaming results in becoming a child of itself
                // in this case we move all the immediate child paths
                // to the newly created path
                let tmp_path = current_path.parent().unwrap().join(nanoid!());
                std::fs::rename(current_path, &tmp_path)?;
                std::fs::create_dir_all(new_path.parent().unwrap())?;
                std::fs::rename(&tmp_path, &new_path)?;
            } else {
                std::fs::create_dir_all(new_path.parent().unwrap())?;
                std::fs::rename(current_path, &new_path)?;
            }

            // Update the paths of in-memory documents
            if new_path.is_file() {
                for doc in ctx.editor.documents.values_mut() {
                    if doc.path.as_ref() == Some(current_path) {
                        doc.path = Some(new_path.clone());
                        break
                    }
                }
            }

            self.close_children();
            self.reset()?;
            self.goto_path(&new_path);
        }

        Ok(EventResult::Consumed(None))
    }

    fn add(&mut self, ctx: &mut Context) -> Result<EventResult> {
        let col = &self.columns[self.active_column];

        let value = self.file_name_input.value();
        let new_path = col.path.join(&value);
        rename_is_valid(&col.path, &new_path, &value)?;


        std::fs::create_dir_all(new_path.parent().unwrap())?;

        if value.ends_with(std::path::MAIN_SEPARATOR) {
            std::fs::create_dir(&new_path)?;
        } else {
            std::fs::write(&new_path, "")?;
        }

        // Update the currently focused doc if its path matches the created path
        // This happens when we open a file, then delete it and create it again
        let mut callback = None;
        if new_path.is_file() {
            let (pane, doc) = current!(ctx.editor);
            let pane_id = pane.id;
            let doc_id = doc.id;
            if doc.path.as_ref() == Some(&new_path) {
                (_, callback) = ctx.editor.sync_pane_changes(pane_id, doc_id);
            }
        }

        self.reset()?;
        self.goto_path(&new_path);
        Ok(EventResult::Consumed(callback))
    }

    fn reposition_cursor(&mut self) {
        for col in self.columns.iter_mut() {
            if col.index >= col.paths.len() {
                col.index = col.paths.len().saturating_sub(1);
            }
        }
    }

    fn refresh_columns(&mut self) -> Result<()> {
        for col in self.columns.iter_mut() {
            col.paths = sorted_entries(&col.path)?;
            self.position_cache.remove(&col.path);
        }

        Ok(())
    }

    fn handle_browsing_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> Result<EventResult> {
        match event.code {
            KeyCode::Esc | KeyCode::Char('-') | KeyCode::Char('q') => {
                if !self.marked_paths.is_empty() {
                    self.marked_paths.clear();
                    return Ok(EventResult::Consumed(None));
                } else if !self.yanked_paths.is_empty() {
                    self.yanked_paths.clear();
                    return Ok(EventResult::Consumed(None));
                }
                Ok(self.dismiss())
            },
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_down();
                Ok(EventResult::Consumed(None))
            },
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_up();
                Ok(EventResult::Consumed(None))
            },
            KeyCode::Char('h') | KeyCode::Left => {
                self.parent()?;
                Ok(EventResult::Consumed(None))
            },
            KeyCode::Char('l') | KeyCode::Right =>  Ok(self.open(ctx, None, false)?),
            KeyCode::Enter => Ok(self.open(ctx, None, true)?),
            KeyCode::Char('g') => {
                self.move_top();
                Ok(EventResult::Consumed(None))
            },
            KeyCode::Char('G') => {
                self.move_bottom();
                Ok(EventResult::Consumed(None))
            },
            KeyCode::Char('v') => {
                if event.modifiers.intersects(KeyModifiers::CONTROL) {
                    Ok(self.open(ctx, Some(Layout::Horizontal), true)?)
                } else {
                    Ok(EventResult::Consumed(None))
                }
            },
            KeyCode::Char('x') => {
                if event.modifiers.intersects(KeyModifiers::CONTROL) {
                    Ok(self.open(ctx, Some(Layout::Vertical), true)?)
                } else {
                    Ok(self.cut(ctx))
                }
            },
            KeyCode::Char('y') => Ok(self.yank(ctx)),
            KeyCode::Char('p') => Ok(self.try_paste()?),
            KeyCode::Char('d') => {
                if event.modifiers.intersects(KeyModifiers::CONTROL) {
                    self.move_half_page_down();
                    Ok(EventResult::Consumed(None))
                } else {
                    Ok(self.try_delete()?)
                }
            },
            KeyCode::Char('u') if event.modifiers.intersects(KeyModifiers::CONTROL) => {
                self.move_half_page_up();
                Ok(EventResult::Consumed(None))
            },
            KeyCode::Char('i') => Ok(self.start_rename(StartRenamingCursorPosition::Start)),
            KeyCode::Char('A') => Ok(self.start_rename(StartRenamingCursorPosition::End)),
            KeyCode::Char('a') => Ok(self.start_rename(StartRenamingCursorPosition::FilenameEnd)),
            KeyCode::Char('c') => Ok(self.start_rename(StartRenamingCursorPosition::FilenameRemoved)),
            KeyCode::Char('C') => Ok(self.start_rename(StartRenamingCursorPosition::NewName)),
            KeyCode::Char('o') => Ok(self.start_add()),
            KeyCode::Char(' ') => Ok(self.mark()),
            KeyCode::Char('/') => {
                self.close_children();
                self.search_input.clear();
                self.state = State::Searching;
                Ok(EventResult::Consumed(None))
            }
            // let the command interface through
            KeyCode::Char(':') => Ok(EventResult::Ignored(None)),
            _ => Ok(EventResult::Consumed(None)),
        }
    }

    fn handle_file_name_input_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> Result<EventResult> {
        match event.code {
            KeyCode::Esc => {
                self.reset()?;
                Ok(EventResult::Consumed(None))
            },
            KeyCode::Char(c) => {
                if self.file_name_input.handle_key_event(event).is_none() {
                    ctx.editor.request_buffered_input(c);
                }
                Ok(EventResult::Consumed(None))
            }
            KeyCode::Enter => match self.state {
                State::Adding => self.add(ctx),
                _ => self.rename(ctx),
            }
            _ => {
                match self.file_name_input.handle_key_event(event) {
                    Some(_) => Ok(EventResult::Consumed(None)),
                    None => Ok(EventResult::Ignored(None)),
                }
            }
        }
    }

    fn handle_searching_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Esc => {
                self.state = State::Browsing;
                EventResult::Consumed(None)
            },
            KeyCode::Char(c) => {
                match self.search_input.handle_key_event(event) {
                    Some(changed) => if changed { self.move_to_first_search_match() },
                    None => ctx.editor.request_buffered_input(c)
                }
                EventResult::Consumed(None)
            }
            KeyCode::Enter => {
                self.move_to_first_search_match();
                self.state = State::Browsing;
                EventResult::Consumed(None)
            }
            _ => {
                match self.search_input.handle_key_event(event) {
                    Some(changed) => {
                        if changed { self.move_to_first_search_match() }
                        EventResult::Consumed(None)
                    }
                    None => EventResult::Ignored(None)
                }
            }
        }
    }

    fn reset(&mut self) -> Result<()> {
        self.state = State::Browsing;
        self.refresh_columns()?;
        self.reposition_cursor();
        self.modal.choice = YesNoCancel::Yes;
        Ok(())
    }

    fn handle_delete_confirmation_key_event(&mut self, event: KeyEvent) -> Result<EventResult> {
        if self.modal.handle_choice(event) {
            if self.modal.choice == YesNoCancel::Yes {
                match &mut self.state {
                    State::ConfirmDelete(paths) => {
                        while let Some(path) = paths.pop() {
                            if let Err(e) = delete_path(&path) {
                                self.close_children();
                                self.reset()?;
                                return Err(e)
                            }
                        }
                    },
                    _ => unreachable!()
                };
            }

            self.close_children();
            self.reset()?
        }

        Ok(EventResult::Consumed(None))
    }

    fn handle_overwrite_confirmation_key_event(&mut self, event: KeyEvent) -> Result<EventResult> {
        if self.modal.handle_choice(event) {
            match self.modal.choice {
                YesNoCancel::Yes => match self.state {
                    State::ConfirmOverwrite(ref path) => {
                        let dest_dir = &self.columns[self.active_column].path;
                        if let Err(e) = self.paste(path, dest_dir) {
                            self.reset()?;
                            return Err(e)
                        }

                        let goto = dest_dir.join(path.file_name().unwrap());
                        self.goto_path(&goto);

                        return self.try_paste();
                    },
                    _ => unreachable!()
                },
                YesNoCancel::No => {
                    return self.try_paste();
                },
                YesNoCancel::Cancel => {
                    self.yanked_paths.clear();
                },
            }

            self.reset()?;
        }

        Ok(EventResult::Consumed(None))
    }

    fn render_active_column(&mut self, idx: usize, short_title: bool, area: Rect, buffer: &mut Buffer) {
        let search_term = &self.search_input.value();
        let selected = &self.marked_paths;
        let yanked = &self.yanked_paths;
        let yank_style = self.paste_action.style();

        let searching = self.state == State::Searching;

        let each_row = |y, path: &Path, inner: Rect, style: Style, buffer: &mut Buffer| {
            // Highlight search matches
            if searching {
                if let Some(path) = path.file_name().and_then(|p| p.to_str()) {
                    if let Some(offset) = path.to_lowercase().find(&search_term.to_lowercase()) {
                        let mut byte = 0;
                        let mut col = 2;
                        for g in path.graphemes(true) {
                            if byte < offset {
                                col += graphemes::width(g);
                                byte += g.len();
                            } else {
                                break;
                            }
                        }
                        let match_area = Rect {
                            position: Position { col: inner.left() + col as u16, row: y },
                            width: graphemes::width(search_term) as u16,
                            height: 1,
                        };
                        buffer.set_style(match_area, style.patch(THEME.get("ui.files.search_match")))
                    }
                }
            }

            // Highlight marked paths
            if selected.contains(path) {
                let highlight_area = Rect {
                    position: Position { col: inner.left(), row: y },
                    width: inner.width,
                    height: 1,
                };

                buffer.set_style(highlight_area, style.patch(THEME.get("ui.files.marked")));
            }

            // Highlight yanked paths
            if yanked.contains(path) {
                let highlight_area = Rect {
                    position: Position { col: inner.left(), row: y },
                    width: inner.width,
                    height: 1,
                };

                buffer.set_style(highlight_area, style.patch(yank_style));
            }
        };

        let column = &mut self.columns[idx];
        let inner = column.render(area, buffer, short_title, each_row);

        if self.state == State::Searching {
            buffer.put_str("󰍉", inner.left(), inner.bottom(), THEME.get("ui.text_input"));
            let mut input_area = inner.clip_left(2).clip_top(inner.height.saturating_sub(1));
            input_area.position.row += 1;
            let mut input_bg = input_area.clip_right(
                input_area.width.saturating_sub(graphemes::width(search_term) as u16 + 2)
            );
            input_bg.position.col = input_bg.position.col.saturating_sub(1);
            buffer.clear(input_bg);
            self.search_input.render(input_area, buffer, None);
        }

        let mut x = inner.right();

        if !selected.is_empty() {
            let count = format!("[{}]", selected.len());
            x = x.saturating_sub(count.len() as u16);
            buffer.put_str(&count, x, inner.bottom(), THEME.get("ui.files.count"));
        }

        if !self.yanked_paths.is_empty() {
            let count = format!("[{}]", self.yanked_paths.len());
            x = x.saturating_sub(count.len() as u16);
            buffer.put_str(&count, x, inner.bottom(), yank_style);
        }
    }

    fn render_inactive_column(&mut self, idx: usize, short_title: bool, area: Rect, buffer: &mut Buffer) {
        let yanked = &self.yanked_paths;
        let yank_style = self.paste_action.style();

        let each_row = |y, path: &Path, inner: Rect, style: Style, buffer: &mut Buffer| {
            // Highlight yanked paths
            if yanked.contains(path) {
                let highlight_area = Rect {
                    position: Position { col: inner.left(), row: y },
                    width: inner.width,
                    height: 1,
                };

                buffer.set_style(highlight_area, style.patch(yank_style));
            }
        };

        self.columns[idx].render(area, buffer, short_title, each_row);
    }

    fn render_file_input(&mut self, buffer: &mut Buffer) {
        let col = &self.columns[self.active_column];
        let path = &col.paths[col.index];
        let cursor = col.scroll.cursor;
        let area = col.calculated_area;
        let area = area.clip_bottom(area.height.saturating_sub(cursor.row));
        let area = area.clip_top(area.height.saturating_sub(1))
            .clip_left(2);
        buffer.clear(area);

        let value = self.file_name_input.value();
        let new_path = col.path.join(&value);
        let style = match rename_is_valid(path, &new_path, &value) {
            Ok(_) => None,
            Err(_) => Some(THEME.get("ui.files.existing")),
        };

        self.file_name_input.render(area, buffer, style);
    }
}

fn rename_is_valid(current_path: &Path, new_path: &Path, new_name: &str) -> Result<()> {
    if new_name.is_empty() {
        bail!("No file name given")
    }

    if new_path != current_path && new_path.exists() {
        bail!("{:?} already exists", new_path.canonicalize().unwrap())
    }

    Ok(())
}

fn next_available_path_name(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap();
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap();
    let mut num = 1;
    let mut new_name = format!("{}-{}", name, num);
    let mut new_path = dir.join(new_name);
    if let Some(ext) = path.extension() {
        new_path.set_extension(ext);
    }

    while new_path.exists() {
        num += 1;
        new_name = format!("{}-{}", name, num);
        new_path = dir.join(new_name)
    }

    new_path
}

// This assumes that all checks have been carried out
// and will overwrite shit.
fn copy_path_to_dir(path: &Path, dest_dir: &Path) -> Result<()> {
    if path.metadata()?.is_dir() {
        let dir_name = path.file_name().unwrap();
        let new_path = dest_dir.join(dir_name);
        recursively_copy_files(path, &new_path)?;
    } else if path.metadata()?.is_file() {
        let file_name = path.file_name().unwrap();
        let to = dest_dir.join(file_name);
        std::fs::copy(path, to)?;
    }

    Ok(())
}

// This assumes all checks are carried out and that
// "from" is not a child of "to" and "from" and "to"
// are not the same
fn recursively_copy_files(from: &Path, to: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(from).into_iter().filter_map(|e| e.ok()) {
        if let Some(entry_str) = entry.path().to_str() {
            if let Some(from_str) = from.to_str() {
                if let Some(to_str) = to.to_str() {
                    let dest = entry_str.replace(from_str, to_str);

                    if entry.path().is_dir() {
                        std::fs::create_dir(dest)?;
                    } else if entry.path().is_file() {
                        std::fs::copy(entry.path(), dest)?;
                    }
                }
            }
        }
    }

    Ok(())
}

// This assumes that all checks have been carried out
// and will overwrite shite.
fn move_path_to_dir(path: &Path, dest_dir: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        // Nothing to do when moving to the same destination
        if parent == dest_dir {
            return Ok(())
        }
    }

    if let Some(fname) = path.file_name().and_then(|p| p.to_str()) {
        let to = dest_dir.join(fname);
        std::fs::rename(path, to)?;
    }

    Ok(())
}

impl Component for Files {
    fn render(&mut self, area: Rect, buffer: &mut Buffer, _ctx: &mut Context) {
        let area = area.clip_bottom(1);

        let available_width = area.width;
        let mut consumed_width = 0;

        let mut to_render = VecDeque::new();

        for column_index in 0..self.columns.len() {
            let active = column_index == self.active_column;

            let width = if active {
                ACTIVE_COLUMN_WIDTH.min(area.width)
            } else {
                INACTIVE_COLUMN_WIDTH
            };

            if column_index <= self.active_column {
                while consumed_width + width > available_width {
                    let (_, Rect { width, .. }) = to_render.pop_front().unwrap();
                    consumed_width -= width;
                    for (_, area) in to_render.iter_mut() {
                        area.position.col -= width;
                    }
                }
            } else if consumed_width + width > available_width { break }

            let column_area = Rect {
                width,
                position: Position {
                    col: consumed_width,
                    ..area.position
                },
                ..area
            };

            to_render.push_back((column_index, column_area));
            consumed_width += width;
        }

        for (i, (idx, area)) in to_render.into_iter().enumerate() {
            if idx == self.active_column {
                self.render_active_column(idx, i != 0, area, buffer);
            } else {
                self.render_inactive_column(idx, i != 0, area, buffer);
            }
        }

        match &self.state {
            State::ConfirmDelete(paths) => {
                if paths.len() > 1 {
                    self.modal.body = format!("Delete {} paths?", paths.len());
                } else {
                    self.modal.body = format!("Delete {}?", cwd_relative_name(paths.first().unwrap()));
                }
                self.modal.render(area, buffer);
            },
            State::ConfirmOverwrite(path) => {
                self.modal.body = format!("Overwrite {}?", path.file_name().and_then(|f| f.to_str()).unwrap());
                self.modal.render(area, buffer);
            },
            State::Adding | State::Renaming(_) => self.render_file_input(buffer),
            _ => {}
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        ctx.editor.status = None;

        match &self.state {
            State::Browsing => {
                self.handle_browsing_key_event(event, ctx).unwrap_or_else(|e| {
                    ctx.editor.set_error(e.to_string());
                    EventResult::Consumed(None)
                })
            },
            State::ConfirmDelete(_) => {
                self.handle_delete_confirmation_key_event(event).unwrap_or_else(|e| {
                    ctx.editor.set_error(e.to_string());
                    EventResult::Consumed(None)
                })
            }
            State::ConfirmOverwrite(_) => {
                self.handle_overwrite_confirmation_key_event(event).unwrap_or_else(|e| {
                    ctx.editor.set_error(e.to_string());
                    EventResult::Consumed(None)
                })
            }
            State::Searching => self.handle_searching_key_event(event, ctx),
            State::Adding | State::Renaming(_) => {
                self.handle_file_name_input_key_event(event, ctx).unwrap_or_else(|e| {
                    ctx.editor.set_error(e.to_string());
                    EventResult::Consumed(None)
                })
            }
        }
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        matches!(self.state, State::ConfirmDelete(_) | State::ConfirmOverwrite(_))
    }

    fn handle_buffered_input(&mut self, string: &str, _ctx: &mut Context) -> EventResult {
        match self.state {
            State::Searching => {
                self.search_input.handle_buffered_input(string);
                self.move_to_first_search_match();
                EventResult::Consumed(None)
            },
            State::Adding | State::Renaming(_) => {
                self.file_name_input.handle_buffered_input(string);
                EventResult::Consumed(None)
            }
            _ => EventResult::Ignored(None)
        }
    }

    fn handle_paste(&mut self, string: &str, ctx: &mut Context) -> EventResult {
        match self.state {
            State::Browsing => {
                let path = PathBuf::from(string);
                if path.exists() {
                    // cannot copy onto itself
                    if !self.columns[self.active_column].path.starts_with(&path) {
                        self.yanked_paths.insert(path);
                        return self.try_paste().unwrap_or_else(|e| {
                            ctx.editor.set_error(e.to_string());
                            EventResult::Consumed(None)
                        })
                    } else {
                        ctx.editor.set_error(format!("Cannot copy {:?} here", path));
                    }
                }

                EventResult::Consumed(None)
            }
            _ => self.handle_buffered_input(string, ctx)
        }
    }

    fn cursor(&self, _area: Rect, _ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        match self.state {
            State::Searching => (Some(self.search_input.scroll.cursor), Some(SetCursorStyle::SteadyBar)),
            State::Adding | State::Renaming(_) => (Some(self.file_name_input.scroll.cursor), Some(SetCursorStyle::SteadyBar)),
            _ => {
                let col = &self.columns[self.active_column];
                let mut cur = col.scroll.cursor;

                if col.paths.get(col.index).is_some() {
                    cur.col += 2;
                }

                (Some(cur), None)
            }
        }
    }

    fn handle_focus_gained(&mut self, ctx: &mut Context) -> EventResult {
        if let Err(e) = self.reset() {
            ctx.editor.set_error(e.to_string());
        }

        EventResult::Consumed(None)
    }
}
