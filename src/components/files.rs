use std::{cmp::Ordering, collections::{BTreeSet, HashMap, VecDeque}, fs::read_dir, path::{Path, PathBuf}};
use anyhow::{anyhow, bail, Result};

use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent, KeyModifiers}};
use unicode_segmentation::UnicodeSegmentation;

use crate::{compositor::{Component, Compositor, Context, EventResult}, current, document::cwd_relative_name, graphemes, language::LANG_CONFIG, panes::Layout, ui::{border_box::BorderBox, borders::Borders, buffer::Buffer, modal::{Choice, Modal}, scroll::Scroll, style::Style, text_input::TextInput, theme::THEME, Position, Rect}};

use super::alert::Alert;

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

#[derive(Debug, PartialEq, Eq)]
enum State {
    Browsing,
    Searching,
    ConfirmingDelete(Vec<PathBuf>),
    ConfirmingOverwrite(Vec<PathBuf>),
}

pub struct Files {
    active_column: usize,
    columns: VecDeque<Column>,
    position_cache: HashMap<PathBuf, PathBuf>,
    marked_paths: BTreeSet<PathBuf>,
    yanked_paths: BTreeSet<PathBuf>,
    yank_source_column: usize,
    paste_action: PasteAction,
    modal: Modal,
    state: State,
    search: TextInput,
}

impl Files {
    pub fn new(path: Option<&PathBuf>) -> Result<Self> {
        let (dir, file) = match path {
            Some(p) => {
                if p.metadata()?.is_dir() {
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
            yank_source_column: 0,
            paste_action: PasteAction::Copy,
            state: State::Browsing,
            modal: Modal::new("⚠ Confirm".into(), "".into()),
            search: TextInput::empty(),
        })
    }

    fn close_children(&mut self) {
        let total = self.columns.len();
        for _ in self.active_column..total.saturating_sub(1) {
            self.columns.pop_back();
        }
        // update position cache
        let col = self.columns.get(self.active_column).unwrap();

        if let Some(p) = col.paths.get(col.index) {
            self.position_cache.insert(col.path.clone(), p.clone());
        }
    }

    fn move_up(&mut self) {
        let col = self.columns.get_mut(self.active_column).unwrap();

        if col.index > 0 {
            col.index -= 1;
            self.close_children();
        }
    }

    fn move_down(&mut self) {
        let col = self.columns.get_mut(self.active_column).unwrap();

        if col.index < col.paths.len().saturating_sub(1) {
            col.index += 1;
            self.close_children();
        }
    }

    fn move_top(&mut self) {
        let col = self.columns.get_mut(self.active_column).unwrap();

        if col.index > 0 {
            col.index = 0;
            self.close_children();
        }
    }

    fn move_bottom(&mut self) {
        let col = self.columns.get_mut(self.active_column).unwrap();

        if col.index < col.paths.len().saturating_sub(1) {
            col.index = col.paths.len().saturating_sub(1);
            self.close_children();
        }
    }

    fn move_to_first_search_match(&mut self) {
        let col = self.columns.get_mut(self.active_column).unwrap();

        for (i, path) in col.paths.iter().enumerate() {
            if let Some(path) = path.file_name().and_then(|p| p.to_str()) {
                if path.contains(&self.search.value()) {
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
            let path = &self.columns.get(self.active_column).unwrap().path;
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
        let col = self.columns.get(self.active_column).unwrap();

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

    fn open(&mut self, ctx: &mut Context, split: Option<Layout>) -> Result<EventResult> {
        if let Selection::File(path) = self.select()? {
            let (pane, _) = current!(ctx.editor);
            let pane_id = pane.id;
            let (hard_wrapped, id) = ctx.editor.open(pane_id, &path)?;
            if let Some(split) = split {
                let doc = ctx.editor.documents.get_mut(&id).unwrap();
                ctx.editor.panes.split(split, doc);
            }
            ctx.editor.panes.load_doc_in_focus(id);
            if hard_wrapped {
                let alert = Alert::new(
                    "⚠ Readonly".into(),
                    format!("The document {:?} is set to Readonly because it contains very long lines which have been hard-wrapped.", path.file_name().unwrap())
                );
                return Ok(EventResult::Consumed(Some(Box::new(|compositor: &mut Compositor, _: &mut Context| {
                    compositor.pop();
                    compositor.push(Box::new(alert));
                }))));
            }
            return Ok(self.dismiss());
        }

        Ok(EventResult::Consumed(None))
    }

    fn mark(&mut self) -> EventResult {
        self.yanked_paths.clear();

        let col = self.columns.get(self.active_column).unwrap();
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
        self.yank_source_column = self.active_column;

        if self.marked_paths.is_empty() {
            let col = self.columns.get(self.active_column).unwrap();
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

    fn paste(&mut self) -> Result<EventResult> {
        if self.yanked_paths.is_empty() {
            bail!("Nothing to paste")
        }

        let dest_dir = &self.columns.get(self.active_column).unwrap().path;
        // This relies on the fact that self.select disallows
        // navigating into directories marked for yanking
        while let Some(path) = self.yanked_paths.pop_first() {
            // These ops are blocking and are running in the main
            // thread so they can block the ui for larger files
            // or large amount of yanked paths
            if let Err(e) = match self.paste_action {
                PasteAction::Copy => copy_path_to_dir(&path, dest_dir),
                PasteAction::Move => move_path_to_dir(&path, dest_dir),
            } {
                self.refresh_columns()?;
                return Err(e);
            }
        }

        self.refresh_columns()?;
        Ok(EventResult::Consumed(None))
    }

    fn try_delete(&mut self) -> Result<EventResult> {
        let mut confirm_paths = vec![];
        if self.marked_paths.is_empty() {
            let col = self.columns.get(self.active_column).unwrap();
            if let Some(marked) = col.paths.get(col.index) {
                confirm_paths.push(marked.to_path_buf());
            }
        } else {
            while let Some(path) = self.marked_paths.pop_first() {
                confirm_paths.push(path);
            }
        }

        if !confirm_paths.is_empty() {
            self.state = State::ConfirmingDelete(confirm_paths)
        }

        Ok(EventResult::Consumed(None))
    }

    fn reposition_cursor(&mut self) {
        let col = self.columns.get_mut(self.active_column).unwrap();

        if col.index >= col.paths.len() {
            col.index = col.paths.len().saturating_sub(1);
        }
    }

    fn refresh_columns(&mut self) -> Result<()> {
        if let Some(col) = self.columns.get_mut(self.yank_source_column) {
            col.paths = sorted_entries(&col.path)?;
            self.position_cache.remove(&col.path);
        }
        if let Some(col) = self.columns.get_mut(self.active_column) {
            col.paths = sorted_entries(&col.path)?;
            self.position_cache.remove(&col.path);
        }

        Ok(())
    }

    fn handle_key_event_when_browsing(&mut self, event: KeyEvent, ctx: &mut Context) -> Result<EventResult> {
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
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter =>  Ok(self.open(ctx, None)?),
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
                    Ok(self.open(ctx, Some(Layout::Horizontal))?)
                } else {
                    Ok(EventResult::Consumed(None))
                }
            },
            KeyCode::Char('x') => {
                if event.modifiers.intersects(KeyModifiers::CONTROL) {
                    Ok(self.open(ctx, Some(Layout::Vertical))?)
                } else {
                    Ok(self.cut(ctx))
                }
            },
            KeyCode::Char('y') => Ok(self.yank(ctx)),
            KeyCode::Char('p') => Ok(self.paste()?),
            KeyCode::Char('d') => Ok(self.try_delete()?),
            KeyCode::Char(' ') => Ok(self.mark()),
            KeyCode::Char('/') => {
                self.close_children();
                self.search.clear();
                self.state = State::Searching;
                Ok(EventResult::Consumed(None))
            }
            // let the command interface through
            KeyCode::Char(':') => Ok(EventResult::Ignored(None)),
            _ => Ok(EventResult::Consumed(None)),
        }
    }

    fn handle_key_event_when_searching(&mut self, event: KeyEvent, _ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Esc | KeyCode::Enter => {
                self.state = State::Browsing;
                EventResult::Consumed(None)
            },
            _ => {
                self.search.handle_key_event(event);
                self.move_to_first_search_match();
                EventResult::Consumed(None)
            }
        }
    }

    fn handle_key_event_when_confirming_delete(&mut self, event: KeyEvent) -> Result<EventResult> {
        if self.modal.confirm(event) {
            if self.modal.choice == Choice::Yes {
                match &mut self.state {
                    State::ConfirmingDelete(paths) => {
                        while let Some(path) = paths.pop() {
                            if let Err(e) = delete_path(&path) {
                                self.refresh_columns()?;
                                return Err(e)
                            }
                        }
                    },
                    _ => unreachable!()
                };

            }

            // reset state
            self.refresh_columns()?;
            self.reposition_cursor();
            self.close_children();
            self.state = State::Browsing;
            self.modal.choice = Choice::Yes;
        }

        Ok(EventResult::Consumed(None))
    }

    fn handle_key_event_when_confirming_overwrite(&mut self, _event: KeyEvent) -> Result<EventResult> {
        Ok(EventResult::Consumed(None))
    }

    fn render_active_column(&mut self, idx: usize, short_title: bool, area: Rect, buffer: &mut Buffer) {
        let search_term = &self.search.value();
        let selected = &self.marked_paths;
        let yanked = &self.yanked_paths;
        let yank_style = self.paste_action.style();

        let each_row = |y, path: &Path, inner: Rect, style: Style, buffer: &mut Buffer| {
            // Highlight search matches
            if let Some(path) = path.file_name().and_then(|p| p.to_str()) {
                if let Some(offset) = path.find(search_term) {
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

        let column = self.columns.get_mut(idx).unwrap();
        let inner = column.render(area, buffer, short_title, each_row);

        if self.state == State::Searching || !search_term.is_empty() {
            buffer.put_str(format!("󰍉 {}", search_term), inner.left(), inner.bottom(), THEME.get("ui.border.files"));
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

        self.columns.get_mut(idx)
            .unwrap()
            .render(area, buffer, short_title, each_row);
    }
}

fn next_available_file_name(path: &Path) -> PathBuf {
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

fn copy_path_to_dir(path: &Path, dest_dir: &Path) -> Result<()> {
    if path.metadata()?.is_dir() {
        copy_dir_to_dir(path, dest_dir)?;
    } else if path.metadata()?.is_file() {
        copy_file_to_dir(path, dest_dir)?;
    }

    Ok(())
}

fn copy_dir_to_dir(path: &Path, dest_dir: &Path) -> Result<()> {
    let dir_name = path.file_name().unwrap();
    let new_path = dest_dir.join(dir_name);

    let destination = if new_path.exists() {
        &next_available_file_name(&new_path)
    } else {
        &new_path
    };

    recursively_copy_files(path, destination)
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

fn copy_file_to_dir(path: &Path, dest_dir: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if parent == dest_dir {
            // When copying to the same parent dir
            // get next available file name and copy
            std::fs::copy(path, next_available_file_name(path))?;
            return Ok(())
        }
    }

    let file_name = path.file_name().unwrap();
    let to = dest_dir.join(file_name);
    if to.exists() {
        bail!("{:?} already exists in {:?}", file_name, dest_dir);
    } else {
        std::fs::copy(path, to)?;
    }

    Ok(())
}

fn move_path_to_dir(path: &Path, dest_dir: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        // Nothing to do when moving to the same destination
        if parent == dest_dir {
            return Ok(())
        }
    }

    if let Some(fname) = path.file_name().and_then(|p| p.to_str()) {
        let to = dest_dir.join(fname);
        if to.try_exists()? {
            bail!("{:?} already exists in {:?}", fname, dest_dir);
        }
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
            State::ConfirmingDelete(paths) => {
                if paths.len() > 1 {
                    self.modal.body = format!("Delete {} paths?", paths.len());
                } else {
                    self.modal.body = format!("Delete {}?", cwd_relative_name(paths.first().unwrap()));
                }
                self.modal.render_all(area, buffer);
            },
            State::ConfirmingOverwrite(paths) => {
                self.modal.body = format!("Overwrite {:?}?", paths);
                self.modal.render_all(area, buffer);
            },
            _ => {}
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        ctx.editor.status = None;

        match &self.state {
            State::Browsing => {
                self.handle_key_event_when_browsing(event, ctx).unwrap_or_else(|e| {
                    ctx.editor.set_error(e.to_string());
                    EventResult::Consumed(None)
                })
            },
            State::ConfirmingDelete(_) => {
                self.handle_key_event_when_confirming_delete(event).unwrap_or_else(|e| {
                    ctx.editor.set_error(e.to_string());
                    EventResult::Consumed(None)
                })
            }
            State::ConfirmingOverwrite(_) => {
                self.handle_key_event_when_confirming_overwrite(event).unwrap_or_else(|e| {
                    ctx.editor.set_error(e.to_string());
                    EventResult::Consumed(None)
                })
            }
            State::Searching => self.handle_key_event_when_searching(event, ctx),
        }
    }

    fn hide_cursor(&self, _ctx: &Context) -> bool {
        self.state != State::Browsing
    }

    fn cursor(&self, _area: Rect, _ctx: &Context) -> (Option<Position>, Option<SetCursorStyle>) {
        let col = self.columns.get(self.active_column).unwrap();
        let mut cur = col.scroll.cursor;

        if col.paths.get(col.index).is_some() {
            cur.col += 2;
        }

        (Some(cur), None)
    }
}
