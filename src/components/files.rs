use std::{cmp::Ordering, collections::{HashMap, VecDeque}, fs::read_dir, path::{Path, PathBuf}};
use anyhow::{anyhow, bail, Result};

use crossterm::{cursor::SetCursorStyle, event::{KeyCode, KeyEvent}};

use crate::{compositor::{Component, Context, EventResult}, language::LANG_CONFIG, ui::{border_box::BorderBox, borders::Borders, buffer::Buffer, scroll::Scroll, style::Style, theme::THEME, Position, Rect}};

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
        ("".into(), THEME.get("ui.files.icon.folder"))
    } else if let Some(config) = LANG_CONFIG.language_config_for_path(path) {
        if let Some(icon) = &config.icon {
            let style = if let Some(c) = &config.color {
                Style::default().fg(*c)
            } else {
                THEME.get("ui.files.icon.file")
            };
            (icon.clone(), style)
        } else {
            ("".into(), THEME.get("ui.files.icon.file"))
        }
    } else {
        ("".into(), THEME.get("ui.files.icon.file"))
    }
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

    fn render(&mut self, mut area: Rect, buffer: &mut Buffer, short_title: bool) {
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

        self.scroll.ensure_point_is_visible(0, self.index, &inner);

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
            }
        }
    }
}

pub struct Files {
    active_column: usize,
    columns: VecDeque<Column>,
    position_cache: HashMap<PathBuf, PathBuf>,
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
                    bail!("Given path is neither a file not a dir")
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

    fn parent(&mut self) -> Result<()> {
        if self.active_column > 0 {
            self.active_column -= 1;
        } else {
            let path = &self.columns.get(self.active_column).unwrap().path;
            if let Some(parent) = path.parent() {
                let col = Column::new(parent.to_path_buf(), Some(path))?;
                self.position_cache.insert(parent.to_path_buf(), path.clone());
                self.columns.push_front(col);
            }
        }

        Ok(())
    }

    fn select(&mut self) -> Result<Selection> {
        let col = self.columns.get(self.active_column).unwrap();

        if let Some(marked) = col.paths.get(col.index) {
            let marked = marked.to_path_buf();
            if marked.metadata()?.is_dir() {
                if self.columns.get(self.active_column + 1).is_none() {
                    let selected = self.position_cache.get(&marked);
                    self.columns.push_back(Column::new(marked.clone(), selected)?);
                }
                self.active_column += 1;
                return Ok(Selection::Dir)
            } else if marked.metadata()?.is_file() {
                return Ok(Selection::File(marked.to_path_buf()))
            }
        }


        Ok(Selection::Invalid)
    }
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
            self.columns.get_mut(idx).unwrap().render(area, buffer, i != 0);
        }
    }

    fn handle_key_event(&mut self, event: KeyEvent, ctx: &mut Context) -> EventResult {
        match event.code {
            KeyCode::Esc | KeyCode::Char('-') | KeyCode::Char('q') => self.dismiss(),
            KeyCode::Char('j') => {
                self.move_down();
                EventResult::Consumed(None)
            },
            KeyCode::Char('k') => {
                self.move_up();
                EventResult::Consumed(None)
            },
            KeyCode::Char('h') => {
                self.parent().unwrap_or_else(|e| ctx.editor.set_error(e.to_string()));
                EventResult::Consumed(None)
            },
            KeyCode::Char('l') | KeyCode::Enter => {
                match self.select() {
                    Ok(Selection::File(file)) => {
                        match ctx.editor.open(&file) {
                            Ok(id) => {
                                ctx.editor.panes.load_doc_in_focus(id);
                                self.dismiss()
                            }
                            Err(e) => {
                                ctx.editor.set_error(e.to_string());
                                EventResult::Consumed(None)
                            }
                        }
                    },
                    Err(e) => {
                        ctx.editor.set_error(e.to_string());
                        EventResult::Consumed(None)
                    },
                    _ => EventResult::Consumed(None)
                }
            },
            KeyCode::Char('g') => {
                self.move_top();
                EventResult::Consumed(None)
            },
            KeyCode::Char('G') => {
                self.move_bottom();
                EventResult::Consumed(None)
            },
            // let the command interface through
            KeyCode::Char(':') => EventResult::Ignored(None),
            _ => EventResult::Consumed(None),
        }
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
