// Mostly copied from helix and treesitter

use std::{
    borrow::Cow, cell::RefCell, collections::{HashMap, VecDeque}, fmt::Write, hash::{Hash, Hasher}, iter::Peekable, mem, ops, path::Path, sync::{atomic::{AtomicUsize, Ordering}, Arc}
};
use ahash::RandomState;
use bitflags::bitflags;
use hashbrown::raw::RawTable;
use slotmap::{new_key_type, HopSlotMap};
use smartstring::{LazyCompact, SmartString};
use crop::{Rope, RopeSlice};
use globset::{Glob, GlobSet, GlobSetBuilder};
use include_dir::{Dir, include_dir};
use once_cell::sync::{Lazy, OnceCell};
use serde::Deserialize;
use tree_sitter::{Language, Node, Parser, Point, Query, QueryCaptures, QueryCursor, QueryError, QueryMatch, Range, TextProvider, Tree};
use regex::Regex;
use unicode_segmentation::UnicodeSegmentation;

use crate::{graphemes::grapheme_is_line_ending, history::Transaction, rope::RopeCursor, ui::theme::THEME};

use super::grammar::get_language;

static QUERIES: Dir = include_dir!("src/language/queries");

pub static LANG_CONFIG: Lazy<Loader> = Lazy::new(|| {
    let config = serde_json::from_str(include_str!("config.json"))
        .expect("Cannot parse language config.json");
    Loader::new(config)
});

fn deserialize_regex<'de, D>(deserializer: D) -> Result<Option<Regex>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)?
        .map(|buf| Regex::new(&buf).map_err(serde::de::Error::custom))
        .transpose()
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Configuration {
    pub languages: Vec<LanguageConfiguration>,
    //#[serde(default)]
    //pub language_server: HashMap<String, LanguageServerConfiguration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LanguageConfiguration {
    #[serde(rename = "name")]
    pub language_id: String, // c-sharp, rust, tsx
    // #[serde(rename = "language-id")]
    // see the table under https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentItem
    // pub language_server_language_id: Option<String>, // csharp, rust, typescriptreact, for the language-server
    // pub scope: String, // source.rust
    pub file_types: Vec<String>, // glob pattern
    #[serde(default)]
    pub shebangs: Vec<String>, // interpreter(s) associated with language
    // #[serde(default)]
    // pub roots: Vec<String>, // these indicate project roots <.git, Cargo.toml>
    // #[serde(
    //     default,
    //     deserialize_with = "from_comment_tokens",
    //     alias = "comment-token"
    // )]
    // pub comment_tokens: Option<Vec<String>>,
    // #[serde(
    //     default,
    //     deserialize_with = "from_block_comment_tokens"
    // )]
    // pub block_comment_tokens: Option<Vec<BlockCommentToken>>,
    // pub text_width: Option<usize>,

    // #[serde(default)]
    // pub auto_format: bool,

    pub icon: Option<String>,

    //pub formatter: Option<FormatterConfiguration>,

    //pub diagnostic_severity: Severity,

    pub grammar: Option<String>, // tree-sitter grammar name, defaults to language_id

    // content_regex
    #[serde(default, deserialize_with = "deserialize_regex")]
    pub injection_regex: Option<Regex>,
    // first_line_regex
    //
    #[serde(skip)]
    pub(crate) highlight_config: OnceCell<Option<Arc<HighlightConfiguration>>>,

    // tags_config OnceCell<> https://github.com/tree-sitter/tree-sitter/pull/583
    //#[serde(
    //    default,
    //    skip_serializing_if = "Vec::is_empty",
    //    deserialize_with = "deserialize_lang_features"
    //)]
    //pub language_servers: Vec<LanguageServerFeatures>,
    // pub indent: Option<IndentationConfiguration>,

    // #[serde(skip)]
    // pub(crate) indent_query: OnceCell<Option<Query>>,
    // #[serde(skip)]
    // pub(crate) textobject_query: OnceCell<Option<TextObjectQuery>>,

    // Automatic insertion of pairs to parentheses, brackets,
    // etc. Defaults to true. Optionally, this can be a list of 2-tuples
    // to specify a list of characters to pair. This overrides the
    // global setting.
    //#[serde(default, deserialize_with = "deserialize_auto_pairs")]
    //pub auto_pairs: Option<AutoPairs>,

    //#[serde(default)]
    //pub persistent_diagnostic_sources: Vec<String>,
}

impl LanguageConfiguration {
    fn initialize_highlight(&self) -> Option<Arc<HighlightConfiguration>> {
        let highlights_query = read_query(&self.language_id, "highlights.scm");
        let injections_query = read_query(&self.language_id, "injections.scm");
        let locals_query = read_query(&self.language_id, "locals.scm");

        if highlights_query.is_empty() {
            None
        } else {
            let language = get_language(self.grammar.as_deref().unwrap_or(&self.language_id))?;
            let mut config = HighlightConfiguration::new(
                language,
                &highlights_query,
                &injections_query,
                &locals_query,
            )
            .map_err(|err| {
                log::error!("Could not parse queries for language {:?}. Consider updating grammar", self.language_id);
                log::error!("This query could not be parsed: {:?}", err);
            })
            .ok()?;

            config.configure();
            Some(Arc::new(config))
        }
    }

    pub fn highlight_config(&self) -> Option<Arc<HighlightConfiguration>> {
        self.highlight_config
            .get_or_init(|| self.initialize_highlight())
            .clone()
    }

    // pub fn indent_query(&self) -> Option<&Query> {
    //     self.indent_query
    //         .get_or_init(|| self.load_query("indents.scm"))
    //         .as_ref()
    // }

    // pub fn textobject_query(&self) -> Option<&TextObjectQuery> {
    //     self.textobject_query
    //         .get_or_init(|| {
    //             self.load_query("textobjects.scm")
    //                 .map(|query| TextObjectQuery { query })
    //         })
    //         .as_ref()
    // }

    // pub fn scope(&self) -> &str {
    //     &self.scope
    // }

    // fn load_query(&self, kind: &str) -> Option<Query> {
    //     let query_text = read_query(&self.language_id, kind);
    //     if query_text.is_empty() {
    //         return None;
    //     }
    //     let lang = &self.highlight_config.get()?.as_ref()?.language;
    //     Query::new(lang, &query_text)
    //         .map_err(|e| {
    //             log::error!(
    //                 "Failed to parse {} queries for {}: {}",
    //                 kind,
    //                 self.language_id,
    //                 e
    //             )
    //         })
    //         .ok()
    // }
}

pub struct Loader {
    language_configs: Vec<Arc<LanguageConfiguration>>,
    matcher: GlobSet,
    file_types: Vec<(Glob, usize)>,
    language_config_ids_by_shebang: HashMap<String, usize>,

    //language_server_configs: HashMap<String, LanguageServerConfiguration>,
}

impl Loader {
    fn new(config: Configuration) -> Self {
        let mut language_configs = Vec::with_capacity(config.languages.len());
        let mut file_types = Vec::with_capacity(language_configs.len());
        let mut language_config_ids_by_shebang = HashMap::new();
        let mut builder = GlobSetBuilder::new();

        for (idx, lang) in config.languages.into_iter().enumerate() {
            for ft in lang.file_types.iter() {
                let glob = Glob::new(ft).unwrap_or_else(|_| { panic!("Invalid glob: {ft}") });
                builder.add(glob.clone());
                file_types.push((glob, idx));
            }

            for shebang in lang.shebangs.iter() {
                language_config_ids_by_shebang.insert(shebang.clone(), idx);
            }

            language_configs.push(Arc::new(lang));
        }

        Self {
            language_configs,
            matcher: builder.build().expect("Cannot build a glob set matcher for file types"),
            file_types,
            language_config_ids_by_shebang,
        }
    }

    pub fn language_config_for_path(&self, path: &Path) -> Option<Arc<LanguageConfiguration>> {
        self.matcher
            .matches(path)
            .iter()
            .filter_map(|idx| self.file_types.get(*idx))
            .max_by_key(|i| i.0.glob().len())
            .map(|i| i.1)
            .and_then(|id| self.language_configs.get(id).cloned())
    }

    pub fn language_config_for_shebang(&self, line: RopeSlice) -> Option<Arc<LanguageConfiguration>> {
        let line = line.chunks().collect::<Cow<_>>();

        static SHEBANG_REGEX: Lazy<Regex> =
            Lazy::new(|| Regex::new(&["^", SHEBANG].concat()).unwrap());

        SHEBANG_REGEX
            .captures(&line)
            .and_then(|cap| self.language_config_ids_by_shebang.get(&cap[1]))
            .and_then(|&id| self.language_configs.get(id).cloned())
    }

    /// Unlike language_config_for_language_id, which only returns Some for an exact id, this
    /// function will perform a regex match on the given string to find the closest language match.
    fn language_config_for_name(&self, name: &str) -> Option<Arc<LanguageConfiguration>> {
        let mut best_match_length = 0;
        let mut best_match_position = None;
        for (i, configuration) in self.language_configs.iter().enumerate() {
            if let Some(injection_regex) = &configuration.injection_regex {
                if let Some(mat) = injection_regex.find(name) {
                    let length = mat.end() - mat.start();
                    if length > best_match_length {
                        best_match_position = Some(i);
                        best_match_length = length;
                    }
                }
            }
        }

        best_match_position.and_then(|id| self.language_configs.get(id).cloned())
    }

    fn language_configuration_for_injection_string(
        &self,
        capture: &InjectionLanguageMarker,
    ) -> Option<Arc<LanguageConfiguration>> {
        match capture {
            InjectionLanguageMarker::Name(string) => self.language_config_for_name(string),
            InjectionLanguageMarker::Filename(file) => self.language_config_for_path(file),
            InjectionLanguageMarker::Shebang(shebang) => self
                .language_config_ids_by_shebang
                .get(shebang)
                .and_then(|&id| self.language_configs.get(id).cloned()),
        }
    }
}

pub struct TsParser {
    parser: tree_sitter::Parser,
    pub cursors: Vec<QueryCursor>,
}

// could also just use a pool, or a single instance?
thread_local! {
    pub static PARSER: RefCell<TsParser> = RefCell::new(TsParser {
        parser: Parser::new(),
        cursors: Vec::new(),
    })
}

new_key_type! {
    pub struct LayerId;
}

fn byte_range_to_str(range: std::ops::Range<usize>, source: RopeSlice) -> Cow<str> {
    source.byte_slice(range).chunks().collect::<Cow<_>>()
}

// #[derive(Debug)]
pub struct Syntax {
    layers: HopSlotMap<LayerId, LanguageLayer>,
    root: LayerId,
}

impl Syntax {
    pub fn new(
        source: Rope,
        config: Arc<HighlightConfiguration>,
    ) -> Option<Self> {
        let root_layer = LanguageLayer {
            tree: None,
            config,
            depth: 0,
            flags: LayerUpdateFlags::empty(),
            ranges: vec![Range {
                start_byte: 0,
                end_byte: usize::MAX,
                start_point: Point::new(0, 0),
                end_point: Point::new(usize::MAX, usize::MAX),
            }],
            parent: None,
        };

        // track scope_descriptor: a Vec of scopes for item in tree

        let mut layers = HopSlotMap::default();
        let root = layers.insert(root_layer);

        let mut syntax = Self {
            root,
            layers,
        };

        let res = syntax.update(source.clone(), source, &Transaction::empty());

        if res.is_err() {
            log::error!("TS parser failed, disabling TS for the current buffer: {res:?}");
            return None;
        }
        Some(syntax)
    }

    pub fn update(
        &mut self,
        old_source: Rope,
        source: Rope,
        transaction: &Transaction,
    ) -> Result<(), Error> {
        let mut queue = VecDeque::new();
        queue.push_back(self.root);

        let injection_callback = |language: &InjectionLanguageMarker| {
            LANG_CONFIG
                .language_configuration_for_injection_string(language)
                .and_then(|language_config| language_config.highlight_config())
        };

        // Convert the changeset into tree sitter edits.
        let edits = generate_edits(old_source, transaction);

        // This table allows inverse indexing of `layers`.
        // That is by hashing a `Layer` you can find
        // the `LayerId` of an existing equivalent `Layer` in `layers`.
        //
        // It is used to determine if a new layer exists for an injection
        // or if an existing layer needs to be updated.
        let mut layers_table = RawTable::with_capacity(self.layers.len());
        let layers_hasher = RandomState::new();
        // Use the edits to update all layers markers
        fn point_add(a: Point, b: Point) -> Point {
            if b.row > 0 {
                Point::new(a.row.saturating_add(b.row), b.column)
            } else {
                Point::new(0, a.column.saturating_add(b.column))
            }
        }
        fn point_sub(a: Point, b: Point) -> Point {
            if a.row > b.row {
                Point::new(a.row.saturating_sub(b.row), a.column)
            } else {
                Point::new(0, a.column.saturating_sub(b.column))
            }
        }

        for (layer_id, layer) in self.layers.iter_mut() {
            // The root layer always covers the whole range (0..usize::MAX)
            if layer.depth == 0 {
                layer.flags = LayerUpdateFlags::MODIFIED;
                continue;
            }

            if !edits.is_empty() {
                for range in &mut layer.ranges {
                    // Roughly based on https://github.com/tree-sitter/tree-sitter/blob/ddeaa0c7f534268b35b4f6cb39b52df082754413/lib/src/subtree.c#L691-L720
                    for edit in edits.iter().rev() {
                        let is_pure_insertion = edit.old_end_byte == edit.start_byte;

                        // if edit is after range, skip
                        if edit.start_byte > range.end_byte {
                            // TODO: || (is_noop && edit.start_byte == range.end_byte)
                            continue;
                        }

                        // if edit is before range, shift entire range by len
                        if edit.old_end_byte < range.start_byte {
                            range.start_byte =
                                edit.new_end_byte + (range.start_byte - edit.old_end_byte);
                            range.start_point = point_add(
                                edit.new_end_position,
                                point_sub(range.start_point, edit.old_end_position),
                            );

                            range.end_byte = edit
                                .new_end_byte
                                .saturating_add(range.end_byte - edit.old_end_byte);
                            range.end_point = point_add(
                                edit.new_end_position,
                                point_sub(range.end_point, edit.old_end_position),
                            );

                            layer.flags |= LayerUpdateFlags::MOVED;
                        }
                        // if the edit starts in the space before and extends into the range
                        else if edit.start_byte < range.start_byte {
                            range.start_byte = edit.new_end_byte;
                            range.start_point = edit.new_end_position;

                            range.end_byte = range
                                .end_byte
                                .saturating_sub(edit.old_end_byte)
                                .saturating_add(edit.new_end_byte);
                            range.end_point = point_add(
                                edit.new_end_position,
                                point_sub(range.end_point, edit.old_end_position),
                            );
                            layer.flags = LayerUpdateFlags::MODIFIED;
                        }
                        // If the edit is an insertion at the start of the tree, shift
                        else if edit.start_byte == range.start_byte && is_pure_insertion {
                            range.start_byte = edit.new_end_byte;
                            range.start_point = edit.new_end_position;
                            layer.flags |= LayerUpdateFlags::MOVED;
                        } else {
                            range.end_byte = range
                                .end_byte
                                .saturating_sub(edit.old_end_byte)
                                .saturating_add(edit.new_end_byte);
                            range.end_point = point_add(
                                edit.new_end_position,
                                point_sub(range.end_point, edit.old_end_position),
                            );
                            layer.flags = LayerUpdateFlags::MODIFIED;
                        }
                    }
                }
            }

            let hash = layers_hasher.hash_one(layer);
            // Safety: insert_no_grow is unsafe because it assumes that the table
            // has enough capacity to hold additional elements.
            // This is always the case as we reserved enough capacity above.
            unsafe { layers_table.insert_no_grow(hash, layer_id) };
        }

        PARSER.with(|ts_parser| {
            let ts_parser = &mut ts_parser.borrow_mut();
            ts_parser.parser.set_timeout_micros(1000 * 500); // half a second is pretty generours
            let mut cursor = ts_parser.cursors.pop().unwrap_or_default();
            // TODO: might need to set cursor range
            cursor.set_byte_range(0..usize::MAX);
            cursor.set_match_limit(TREE_SITTER_MATCH_LIMIT);

            while let Some(layer_id) = queue.pop_front() {
                let source_slice = source.byte_slice(..);

                let layer = &mut self.layers[layer_id];

                // Mark the layer as touched
                layer.flags |= LayerUpdateFlags::TOUCHED;

                // If a tree already exists, notify it of changes.
                if let Some(tree) = &mut layer.tree {
                    if layer
                        .flags
                        .intersects(LayerUpdateFlags::MODIFIED | LayerUpdateFlags::MOVED)
                    {
                        for edit in edits.iter().rev() {
                            // Apply the edits in reverse.
                            // If we applied them in order then edit 1 would disrupt the positioning of edit 2.
                            tree.edit(edit);
                        }
                    }

                    if layer.flags.contains(LayerUpdateFlags::MODIFIED) {
                        // Re-parse the tree.
                        layer.parse(&mut ts_parser.parser, source_slice)?;
                    }
                } else {
                    // always parse if this layer has never been parsed before
                    layer.parse(&mut ts_parser.parser, source_slice)?;
                }

                // Switch to an immutable borrow.
                let layer = &self.layers[layer_id];

                // Process injections.
                let matches = cursor.matches(
                    &layer.config.injections_query,
                    layer.tree().root_node(),
                    RopeProvider(source_slice),
                );
                let mut combined_injections = vec![
                    (None, Vec::new(), IncludedChildren::default());
                    layer.config.combined_injections_patterns.len()
                ];
                let mut injections = Vec::new();
                let mut last_injection_end = 0;
                for mat in matches {
                    let (injection_capture, content_node, included_children) = layer
                        .config
                        .injection_for_match(&layer.config.injections_query, &mat, source_slice);

                    // in case this is a combined injection save it for more processing later
                    if let Some(combined_injection_idx) = layer
                        .config
                        .combined_injections_patterns
                        .iter()
                        .position(|&pattern| pattern == mat.pattern_index)
                    {
                        let entry = &mut combined_injections[combined_injection_idx];
                        if injection_capture.is_some() {
                            entry.0 = injection_capture;
                        }
                        if let Some(content_node) = content_node {
                            if content_node.start_byte() >= last_injection_end {
                                entry.1.push(content_node);
                                last_injection_end = content_node.end_byte();
                            }
                        }
                        entry.2 = included_children;
                        continue;
                    }

                    // Explicitly remove this match so that none of its other captures will remain
                    // in the stream of captures.
                    mat.remove();

                    // If a language is found with the given name, then add a new language layer
                    // to the highlighted document.
                    if let (Some(injection_capture), Some(content_node)) =
                        (injection_capture, content_node)
                    {
                        if let Some(config) = (injection_callback)(&injection_capture) {
                            let ranges =
                                intersect_ranges(&layer.ranges, &[content_node], included_children);

                            if !ranges.is_empty() {
                                if content_node.start_byte() < last_injection_end {
                                    continue;
                                }
                                last_injection_end = content_node.end_byte();
                                injections.push((config, ranges));
                            }
                        }
                    }
                }

                for (lang_name, content_nodes, included_children) in combined_injections {
                    if let (Some(lang_name), false) = (lang_name, content_nodes.is_empty()) {
                        if let Some(config) = (injection_callback)(&lang_name) {
                            let ranges =
                                intersect_ranges(&layer.ranges, &content_nodes, included_children);
                            if !ranges.is_empty() {
                                injections.push((config, ranges));
                            }
                        }
                    }
                }

                let depth = layer.depth + 1;
                // TODO: can't inline this since matches borrows self.layers
                for (config, ranges) in injections {
                    let parent = Some(layer_id);
                    let new_layer = LanguageLayer {
                        tree: None,
                        config,
                        depth,
                        ranges,
                        flags: LayerUpdateFlags::empty(),
                        parent: None,
                    };

                    // Find an identical existing layer
                    let layer = layers_table
                        .get(layers_hasher.hash_one(&new_layer), |&it| {
                            self.layers[it] == new_layer
                        })
                        .copied();

                    // ...or insert a new one.
                    let layer_id = layer.unwrap_or_else(|| self.layers.insert(new_layer));
                    self.layers[layer_id].parent = parent;

                    queue.push_back(layer_id);
                }

                // TODO: pre-process local scopes at this time, rather than highlight?
                // would solve problems with locals not working across boundaries
            }

            // Return the cursor back in the pool.
            ts_parser.cursors.push(cursor);

            // Reset all `LayerUpdateFlags` and remove all untouched layers
            self.layers.retain(|_, layer| {
                mem::replace(&mut layer.flags, LayerUpdateFlags::empty())
                    .contains(LayerUpdateFlags::TOUCHED)
            });

            Ok(())
        })
    }

    // pub fn tree(&self) -> &Tree {
    //     self.layers[self.root].tree()
    // }

    /// Iterate over the highlighted regions for a given slice of source code.
    pub fn highlight_iter<'a>(
        &'a self,
        source: RopeSlice<'a>,
        range: Option<std::ops::Range<usize>>,
        cancellation_flag: Option<&'a AtomicUsize>,
    ) -> impl Iterator<Item = Result<HighlightEvent, Error>> + 'a {
        let mut layers = self
            .layers
            .iter()
            .filter_map(|(_, layer)| {
                // TODO: if range doesn't overlap layer range, skip it

                // Reuse a cursor from the pool if available.
                let mut cursor = PARSER.with(|ts_parser| {
                    let highlighter = &mut ts_parser.borrow_mut();
                    highlighter.cursors.pop().unwrap_or_default()
                });

                // The `captures` iterator borrows the `Tree` and the `QueryCursor`, which
                // prevents them from being moved. But both of these values are really just
                // pointers, so it's actually ok to move them.
                let cursor_ref = unsafe {
                    mem::transmute::<&mut tree_sitter::QueryCursor, &mut tree_sitter::QueryCursor>(
                        &mut cursor,
                    )
                };

                // if reusing cursors & no range this resets to whole range
                cursor_ref.set_byte_range(range.clone().unwrap_or(0..usize::MAX));
                cursor_ref.set_match_limit(TREE_SITTER_MATCH_LIMIT);

                let mut captures = cursor_ref
                    .captures(
                        &layer.config.query,
                        layer.tree().root_node(),
                        RopeProvider(source),
                    )
                    .peekable();

                // If there's no captures, skip the layer
                captures.peek()?;

                Some(HighlightIterLayer {
                    highlight_end_stack: Vec::new(),
                    scope_stack: vec![LocalScope {
                        inherits: false,
                        range: 0..usize::MAX,
                        local_defs: Vec::new(),
                    }],
                    cursor,
                    _tree: None,
                    captures: RefCell::new(captures),
                    config: layer.config.as_ref(),
                    depth: layer.depth,
                })
            })
            .collect::<Vec<_>>();

        layers.sort_unstable_by_key(|layer| layer.sort_key());

        let mut result = HighlightIter {
            source,
            byte_offset: range.map_or(0, |r| r.start),
            cancellation_flag,
            iter_count: 0,
            layers,
            next_event: None,
            last_highlight_range: None,
        };
        result.sort_layers();
        result
    }

    // pub fn tree_for_byte_range(&self, start: usize, end: usize) -> &Tree {
    //     let mut container_id = self.root;
    //
    //     for (layer_id, layer) in self.layers.iter() {
    //         if layer.depth > self.layers[container_id].depth
    //             && layer.contains_byte_range(start, end)
    //         {
    //             container_id = layer_id;
    //         }
    //     }
    //
    //     self.layers[container_id].tree()
    // }

    // pub fn named_descendant_for_byte_range(&self, start: usize, end: usize) -> Option<Node<'_>> {
    //     self.tree_for_byte_range(start, end)
    //         .root_node()
    //         .named_descendant_for_byte_range(start, end)
    // }

    // pub fn descendant_for_byte_range(&self, start: usize, end: usize) -> Option<Node<'_>> {
    //     self.tree_for_byte_range(start, end)
    //         .root_node()
    //         .descendant_for_byte_range(start, end)
    // }

    // pub fn walk(&self) -> TreeCursor<'_> {
    //     // data structure to find the smallest range that contains a point
    //     // when some of the ranges in the structure can overlap.
    //     TreeCursor::new(&self.layers, self.root)
    // }

    // Commenting
    // comment_strings_for_pos
    // is_commented

    // Indentation
    // suggested_indent_for_line_at_buffer_row
    // suggested_indent_for_buffer_row
    // indent_level_for_line

    // TODO: Folding
}

bitflags! {
    /// Flags that track the status of a layer
    /// in the `Sytaxn::update` function
    #[derive(Debug)]
    struct LayerUpdateFlags : u32{
        const MODIFIED = 0b001;
        const MOVED = 0b010;
        const TOUCHED = 0b100;
    }
}

#[derive(Debug)]
pub struct LanguageLayer {
    // mode
    // grammar
    pub config: Arc<HighlightConfiguration>,
    pub(crate) tree: Option<Tree>,
    pub ranges: Vec<Range>,
    pub depth: u32,
    flags: LayerUpdateFlags,
    parent: Option<LayerId>,
}

/// This PartialEq implementation only checks if that
/// two layers are theoretically identical (meaning they highlight the same text range with the same language).
/// It does not check whether the layers have the same internal treesitter
/// state.
impl PartialEq for LanguageLayer {
    fn eq(&self, other: &Self) -> bool {
        self.depth == other.depth
            && self.config.language == other.config.language
            && self.ranges == other.ranges
    }
}

/// Hash implementation belongs to PartialEq implementation above.
/// See its documentation for details.
impl Hash for LanguageLayer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.depth.hash(state);
        self.config.language.hash(state);
        self.ranges.hash(state);
    }
}

impl LanguageLayer {
    fn tree(&self) -> &Tree {
        // TODO: no unwrap
        self.tree.as_ref().unwrap()
    }

    fn parse(&mut self, parser: &mut Parser, source: RopeSlice) -> Result<(), Error> {
        parser
            .set_included_ranges(&self.ranges)
            .map_err(|_| Error::InvalidRanges)?;

        parser
            .set_language(&self.config.language)
            .map_err(|_| Error::InvalidLanguage)?;

        // unsafe { syntax.parser.set_cancellation_flag(cancellation_flag) };
        // Can't use parse_with here because crop::Rope doesn't allow getting
        // chunks by byte index
        let tree = parser.parse(source.to_string(), self.tree.as_ref())
            .ok_or(Error::Cancelled)?;
        // unsafe { ts_parser.parser.set_cancellation_flag(None) };
        self.tree = Some(tree);
        Ok(())
    }

    // Whether the layer contains the given byte range.
    //
    // If the layer has multiple ranges (i.e. combined injections), the
    // given range is considered contained if it is within the start and
    // end bytes of the first and last ranges **and** if the given range
    // starts or ends within any of the layer's ranges.
    // fn contains_byte_range(&self, start: usize, end: usize) -> bool {
    //     let layer_start = self
    //         .ranges
    //         .first()
    //         .expect("ranges should not be empty")
    //         .start_byte;
    //     let layer_end = self
    //         .ranges
    //         .last()
    //         .expect("ranges should not be empty")
    //         .end_byte;
    //
    //     layer_start <= start
    //         && layer_end >= end
    //         && self.ranges.iter().any(|range| {
    //             let byte_range = range.start_byte..range.end_byte;
    //             byte_range.contains(&start) || byte_range.contains(&end)
    //         })
    // }
}

fn generate_edits(
    old_text: Rope,
    transaction: &Transaction,
) -> Vec<tree_sitter::InputEdit> {
    use crate::history::Operation::*;
    let mut old_byte = 0;

    let mut edits = Vec::new();

    if transaction.is_empty() {
        return edits;
    }

    let mut iter = transaction.operations.iter().peekable();

    fn point_at_byte(text: &Rope, byte: usize) -> Point {
        let line = text.line_of_byte(byte);
        let line_start = text.byte_of_line(line);
        let col = byte - line_start;

        Point::new(line, col)
    }

    fn traverse(point: Point, text: &SmartString<LazyCompact>) -> Point {
        let Point {
            mut row,
            mut column,
        } = point;

        for g in text.graphemes(true) {
            if grapheme_is_line_ending(g) {
                row += 1;
                column = 0;
            } else {
                column += g.len();
            }
        }
        Point { row, column }
    }

    while let Some(operation) = iter.next() {
        let len = match operation {
            Delete(i) | Retain(i) => *i,
            Insert(_) => 0,
        };
        let mut old_end_byte = old_byte + len;

        match operation {
            Retain(_) => {}
            Delete(_) => {
                let start_position = point_at_byte(&old_text, old_byte);
                let old_end_position = point_at_byte(&old_text, old_end_byte);

                // deletion
                edits.push(tree_sitter::InputEdit {
                    start_byte: old_byte,
                    old_end_byte,
                    new_end_byte: old_byte,
                    start_position,
                    old_end_position,
                    new_end_position: start_position,
                });
            }
            Insert(s) => {
                let start_position = point_at_byte(&old_text, old_byte);

                // a subsequent delete means a replace, consume it
                if let Some(Delete(len)) = iter.peek() {
                    old_end_byte = old_byte + len;
                    let old_end_position = point_at_byte(&old_text, old_end_byte);

                    iter.next();

                    // replacement
                    edits.push(tree_sitter::InputEdit {
                        start_byte: old_byte,
                        old_end_byte,
                        new_end_byte: old_byte + s.len(),
                        start_position,
                        old_end_position,
                        new_end_position: traverse(start_position, s), // old pos + chars, newlines matter too (iter over)
                    });
                } else {
                    // insert
                    edits.push(tree_sitter::InputEdit {
                        start_byte: old_byte,
                        old_end_byte: old_byte,
                        new_end_byte: old_byte + s.len(),
                        start_position,
                        old_end_position: start_position,
                        new_end_position: traverse(start_position, s),
                    });
                }
            }
        }
        old_byte = old_end_byte;
    }
    edits
}

const CANCELLATION_CHECK_INTERVAL: usize = 100;

/// Indicates which highlight should be applied to a region of source code.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Highlight(pub usize);

/// Represents the reason why syntax highlighting failed.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    InvalidLanguage,
    InvalidRanges,
    // Unknown,
}

/// Represents a single step in rendering a syntax-highlighted document.
#[derive(Copy, Clone, Debug)]
pub enum HighlightEvent {
    Source { start: usize, end: usize },
    HighlightStart(Highlight),
    HighlightEnd,
}

/// Contains the data needed to highlight code written in a particular language.
///
/// This struct is immutable and can be shared between threads.
#[derive(Debug)]
pub struct HighlightConfiguration {
    pub language: Language,
    pub query: Query,
    injections_query: Query,
    combined_injections_patterns: Vec<usize>,
    highlights_pattern_index: usize,
    highlight_indices: Vec<Option<Highlight>>,
    non_local_variable_patterns: Vec<bool>,
    injection_content_capture_index: Option<u32>,
    injection_language_capture_index: Option<u32>,
    injection_filename_capture_index: Option<u32>,
    injection_shebang_capture_index: Option<u32>,
    local_scope_capture_index: Option<u32>,
    local_def_capture_index: Option<u32>,
    local_def_value_capture_index: Option<u32>,
    local_ref_capture_index: Option<u32>,
}

#[derive(Debug)]
struct LocalDef<'a> {
    name: Cow<'a, str>,
    value_range: ops::Range<usize>,
    highlight: Option<Highlight>,
}

#[derive(Debug)]
struct LocalScope<'a> {
    inherits: bool,
    range: ops::Range<usize>,
    local_defs: Vec<LocalDef<'a>>,
}

// #[derive(Debug)]
pub struct HighlightIter<'a> {
    source: RopeSlice<'a>,
    byte_offset: usize,
    cancellation_flag: Option<&'a AtomicUsize>,
    layers: Vec<HighlightIterLayer<'a>>,
    iter_count: usize,
    next_event: Option<HighlightEvent>,
    last_highlight_range: Option<(usize, usize, u32)>,
}

impl HighlightIter<'_> {
    fn emit_event(
        &mut self,
        offset: usize,
        event: Option<HighlightEvent>,
    ) -> Option<Result<HighlightEvent, Error>> {
        let result;
        if self.byte_offset < offset {
            result = Some(Ok(HighlightEvent::Source {
                start: self.byte_offset,
                end: offset,
            }));
            self.byte_offset = offset;
            self.next_event = event;
        } else {
            result = event.map(Ok);
        }
        self.sort_layers();
        result
    }

    fn sort_layers(&mut self) {
        while !self.layers.is_empty() {
            if let Some(sort_key) = self.layers[0].sort_key() {
                let mut i = 0;
                while i + 1 < self.layers.len() {
                    if let Some(next_offset) = self.layers[i + 1].sort_key() {
                        if next_offset < sort_key {
                            i += 1;
                            continue;
                        }
                    } else {
                        let layer = self.layers.remove(i + 1);
                        PARSER.with(|ts_parser| {
                            let highlighter = &mut ts_parser.borrow_mut();
                            highlighter.cursors.push(layer.cursor);
                        });
                    }
                    break;
                }
                if i > 0 {
                    self.layers[0..(i + 1)].rotate_left(1);
                }
                break;
            } else {
                let layer = self.layers.remove(0);
                PARSER.with(|ts_parser| {
                    let highlighter = &mut ts_parser.borrow_mut();
                    highlighter.cursors.push(layer.cursor);
                });
            }
        }
    }
}

impl Iterator for HighlightIter<'_> {
    type Item = Result<HighlightEvent, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        'main: loop {
            // If we've already determined the next highlight boundary, just return it.
            if let Some(e) = self.next_event.take() {
                return Some(Ok(e));
            }

            // Periodically check for cancellation, returning `Cancelled` error if the
            // cancellation flag was flipped.
            if let Some(cancellation_flag) = self.cancellation_flag {
                self.iter_count += 1;
                if self.iter_count >= CANCELLATION_CHECK_INTERVAL {
                    self.iter_count = 0;
                    if cancellation_flag.load(Ordering::Relaxed) != 0 {
                        return Some(Err(Error::Cancelled));
                    }
                }
            }

            // If none of the layers have any more highlight boundaries, terminate.
            if self.layers.is_empty() {
                let len = self.source.byte_len();
                return if self.byte_offset < len {
                    let result = Some(Ok(HighlightEvent::Source {
                        start: self.byte_offset,
                        end: len,
                    }));
                    self.byte_offset = len;
                    result
                } else {
                    None
                };
            }

            // Get the next capture from whichever layer has the earliest highlight boundary.
            let range;
            let layer = &mut self.layers[0];
            let captures = layer.captures.get_mut();
            if let Some((next_match, capture_index)) = captures.peek() {
                let next_capture = next_match.captures[*capture_index];
                range = next_capture.node.byte_range();

                // If any previous highlight ends before this node starts, then before
                // processing this capture, emit the source code up until the end of the
                // previous highlight, and an end event for that highlight.
                if let Some(end_byte) = layer.highlight_end_stack.last().cloned() {
                    if end_byte <= range.start {
                        layer.highlight_end_stack.pop();
                        return self.emit_event(end_byte, Some(HighlightEvent::HighlightEnd));
                    }
                }
            }
            // If there are no more captures, then emit any remaining highlight end events.
            // And if there are none of those, then just advance to the end of the document.
            else if let Some(end_byte) = layer.highlight_end_stack.last().cloned() {
                layer.highlight_end_stack.pop();
                return self.emit_event(end_byte, Some(HighlightEvent::HighlightEnd));
            } else {
                return self.emit_event(self.source.byte_len(), None);
            };

            let (mut match_, capture_index) = captures.next().unwrap();
            let mut capture = match_.captures[capture_index];

            // Remove from the local scope stack any local scopes that have already ended.
            while range.start > layer.scope_stack.last().unwrap().range.end {
                layer.scope_stack.pop();
            }

            // If this capture is for tracking local variables, then process the
            // local variable info.
            let mut reference_highlight = None;
            let mut definition_highlight = None;
            while match_.pattern_index < layer.config.highlights_pattern_index {
                // If the node represents a local scope, push a new local scope onto
                // the scope stack.
                if Some(capture.index) == layer.config.local_scope_capture_index {
                    definition_highlight = None;
                    let mut scope = LocalScope {
                        inherits: true,
                        range: range.clone(),
                        local_defs: Vec::new(),
                    };
                    for prop in layer.config.query.property_settings(match_.pattern_index) {
                        if let "local.scope-inherits" = prop.key.as_ref() {
                            scope.inherits =
                                prop.value.as_ref().map_or(true, |r| r.as_ref() == "true");
                        }
                    }
                    layer.scope_stack.push(scope);
                }
                // If the node represents a definition, add a new definition to the
                // local scope at the top of the scope stack.
                else if Some(capture.index) == layer.config.local_def_capture_index {
                    reference_highlight = None;
                    let scope = layer.scope_stack.last_mut().unwrap();

                    let mut value_range = 0..0;
                    for capture in match_.captures {
                        if Some(capture.index) == layer.config.local_def_value_capture_index {
                            value_range = capture.node.byte_range();
                        }
                    }

                    let name = byte_range_to_str(range.clone(), self.source);
                    scope.local_defs.push(LocalDef {
                        name,
                        value_range,
                        highlight: None,
                    });
                    definition_highlight = scope.local_defs.last_mut().map(|s| &mut s.highlight);
                }
                // If the node represents a reference, then try to find the corresponding
                // definition in the scope stack.
                else if Some(capture.index) == layer.config.local_ref_capture_index
                    && definition_highlight.is_none()
                {
                    definition_highlight = None;
                    let name = byte_range_to_str(range.clone(), self.source);
                    for scope in layer.scope_stack.iter().rev() {
                        if let Some(highlight) = scope.local_defs.iter().rev().find_map(|def| {
                            if def.name == name && range.start >= def.value_range.end {
                                Some(def.highlight)
                            } else {
                                None
                            }
                        }) {
                            reference_highlight = highlight;
                            break;
                        }
                        if !scope.inherits {
                            break;
                        }
                    }
                }

                // Continue processing any additional matches for the same node.
                if let Some((next_match, next_capture_index)) = captures.peek() {
                    let next_capture = next_match.captures[*next_capture_index];
                    if next_capture.node == capture.node {
                        capture = next_capture;
                        match_ = captures.next().unwrap().0;
                        continue;
                    }
                }

                self.sort_layers();
                continue 'main;
            }

            // Otherwise, this capture must represent a highlight.
            // If this exact range has already been highlighted by an earlier pattern, or by
            // a different layer, then skip over this one.
            if let Some((last_start, last_end, last_depth)) = self.last_highlight_range {
                if range.start == last_start && range.end == last_end && layer.depth < last_depth {
                    self.sort_layers();
                    continue 'main;
                }
            }

            // If the current node was found to be a local variable, then skip over any
            // highlighting patterns that are disabled for local variables.
            if definition_highlight.is_some() || reference_highlight.is_some() {
                while layer.config.non_local_variable_patterns[match_.pattern_index] {
                    match_.remove();
                    if let Some((next_match, next_capture_index)) = captures.peek() {
                        let next_capture = next_match.captures[*next_capture_index];
                        if next_capture.node == capture.node {
                            capture = next_capture;
                            match_ = captures.next().unwrap().0;
                            continue;
                        }
                    }

                    self.sort_layers();
                    continue 'main;
                }
            }

            // Once a highlighting pattern is found for the current node, skip over
            // any later highlighting patterns that also match this node. Captures
            // for a given node are ordered by pattern index, so these subsequent
            // captures are guaranteed to be for highlighting, not injections or
            // local variables.
            while let Some((next_match, next_capture_index)) = captures.peek() {
                let next_capture = next_match.captures[*next_capture_index];
                if next_capture.node == capture.node {
                    captures.next();
                } else {
                    break;
                }
            }

            let current_highlight = layer.config.highlight_indices[capture.index as usize];

            // If this node represents a local definition, then store the current
            // highlight value on the local scope entry representing this node.
            if let Some(definition_highlight) = definition_highlight {
                *definition_highlight = current_highlight;
            }

            // Emit a scope start event and push the node's end position to the stack.
            if let Some(highlight) = reference_highlight.or(current_highlight) {
                self.last_highlight_range = Some((range.start, range.end, layer.depth));
                layer.highlight_end_stack.push(range.end);
                return self
                    .emit_event(range.start, Some(HighlightEvent::HighlightStart(highlight)));
            }

            self.sort_layers();
        }
    }
}

struct HighlightIterLayer<'a> {
    _tree: Option<Tree>,
    cursor: QueryCursor,
    captures: RefCell<Peekable<QueryCaptures<'a, 'a, RopeProvider<'a>, &'a [u8]>>>,
    config: &'a HighlightConfiguration,
    highlight_end_stack: Vec<usize>,
    scope_stack: Vec<LocalScope<'a>>,
    depth: u32,
}

impl HighlightIterLayer<'_> {
    // First, sort scope boundaries by their byte offset in the document. At a
    // given position, emit scope endings before scope beginnings. Finally, emit
    // scope boundaries from deeper layers first.
    fn sort_key(&self) -> Option<(usize, bool, isize)> {
        let depth = -(self.depth as isize);
        let next_start = self
            .captures
            .borrow_mut()
            .peek()
            .map(|(m, i)| m.captures[*i].node.start_byte());
        let next_end = self.highlight_end_stack.last().cloned();
        match (next_start, next_end) {
            (Some(start), Some(end)) => {
                if start < end {
                    Some((start, true, depth))
                } else {
                    Some((end, false, depth))
                }
            }
            (Some(i), None) => Some((i, true, depth)),
            (None, Some(j)) => Some((j, false, depth)),
            _ => None,
        }
    }
}

impl HighlightConfiguration {
    /// Creates a `HighlightConfiguration` for a given `Grammar` and set of highlighting
    /// queries.
    ///
    /// # Parameters
    ///
    /// * `language`  - The Tree-sitter `Grammar` that should be used for parsing.
    /// * `highlights_query` - A string containing tree patterns for syntax highlighting. This
    ///   should be non-empty, otherwise no syntax highlights will be added.
    /// * `injections_query` -  A string containing tree patterns for injecting other languages
    ///   into the document. This can be empty if no injections are desired.
    /// * `locals_query` - A string containing tree patterns for tracking local variable
    ///   definitions and references. This can be empty if local variable tracking is not needed.
    ///
    /// Returns a `HighlightConfiguration` that can then be used with the `highlight` method.
    pub fn new(
        language: Language,
        highlights_query: &str,
        injection_query: &str,
        locals_query: &str,
    ) -> Result<Self, QueryError> {
        // Concatenate the query strings, keeping track of the start offset of each section.
        let mut query_source = String::new();
        query_source.push_str(locals_query);
        let highlights_query_offset = query_source.len();
        query_source.push_str(highlights_query);

        // Construct a single query by concatenating the three query strings, but record the
        // range of pattern indices that belong to each individual string.
        let query = Query::new(&language, &query_source)?;
        let mut highlights_pattern_index = 0;
        for i in 0..(query.pattern_count()) {
            let pattern_offset = query.start_byte_for_pattern(i);
            if pattern_offset < highlights_query_offset {
                highlights_pattern_index += 1;
            }
        }

        let injections_query = Query::new(&language, injection_query)?;
        let combined_injections_patterns = (0..injections_query.pattern_count())
            .filter(|&i| {
                injections_query
                    .property_settings(i)
                    .iter()
                    .any(|s| &*s.key == "injection.combined")
            })
            .collect();

        // Find all of the highlighting patterns that are disabled for nodes that
        // have been identified as local variables.
        let non_local_variable_patterns = (0..query.pattern_count())
            .map(|i| {
                query
                    .property_predicates(i)
                    .iter()
                    .any(|(prop, positive)| !*positive && prop.key.as_ref() == "local")
            })
            .collect();

        // Store the numeric ids for all of the special captures.
        let mut injection_content_capture_index = None;
        let mut injection_language_capture_index = None;
        let mut injection_filename_capture_index = None;
        let mut injection_shebang_capture_index = None;
        let mut local_def_capture_index = None;
        let mut local_def_value_capture_index = None;
        let mut local_ref_capture_index = None;
        let mut local_scope_capture_index = None;
        for (i, name) in query.capture_names().iter().enumerate() {
            let i = Some(i as u32);
            match *name {
                "local.definition" => local_def_capture_index = i,
                "local.definition-value" => local_def_value_capture_index = i,
                "local.reference" => local_ref_capture_index = i,
                "local.scope" => local_scope_capture_index = i,
                _ => {}
            }
        }

        for (i, name) in injections_query.capture_names().iter().enumerate() {
            let i = Some(i as u32);
            match *name {
                "injection.content" => injection_content_capture_index = i,
                "injection.language" => injection_language_capture_index = i,
                "injection.filename" => injection_filename_capture_index = i,
                "injection.shebang" => injection_shebang_capture_index = i,
                _ => {}
            }
        }

        let highlight_indices = vec![None; query.capture_names().len()];
        Ok(Self {
            language,
            query,
            injections_query,
            combined_injections_patterns,
            highlights_pattern_index,
            highlight_indices,
            non_local_variable_patterns,
            injection_content_capture_index,
            injection_language_capture_index,
            injection_filename_capture_index,
            injection_shebang_capture_index,
            local_scope_capture_index,
            local_def_capture_index,
            local_def_value_capture_index,
            local_ref_capture_index,
        })
    }

    // Get a slice containing all of the highlight names used in the configuration.
    // pub fn names(&self) -> &[&str] {
    //     self.query.capture_names()
    // }

    /// Set the list of recognized highlight names.
    ///
    /// Tree-sitter syntax-highlighting queries specify highlights in the form of dot-separated
    /// highlight names like `punctuation.bracket` and `function.method.builtin`. Consumers of
    /// these queries can choose to recognize highlights with different levels of specificity.
    /// For example, the string `function.builtin` will match against `function.builtin.constructor`
    /// but will not match `function.method.builtin` and `function.method`.
    ///
    /// When highlighting, results are returned as `Highlight` values, which contain the index
    /// of the matched highlight this list of highlight names.
    pub fn configure(&mut self) {
        let mut capture_parts = Vec::new();
        let indices: Vec<_> = self
            .query
            .capture_names()
            .iter()
            .map(move |capture_name| {
                capture_parts.clear();
                capture_parts.extend(capture_name.split('.'));

                let mut best_index = None;
                let mut best_match_len = 0;
                for (i, recognized_name) in THEME.scopes.iter().enumerate() {
                    let mut len = 0;
                    let mut matches = true;
                    for (i, part) in recognized_name.split('.').enumerate() {
                        match capture_parts.get(i) {
                            Some(capture_part) if *capture_part == part => len += 1,
                            _ => {
                                matches = false;
                                break;
                            }
                        }
                    }
                    if matches && len > best_match_len {
                        best_index = Some(i);
                        best_match_len = len;
                    }
                }
                best_index.map(Highlight)
            })
            .collect();

        self.highlight_indices = indices;
    }

    fn injection_pair<'a>(
        &self,
        query_match: &QueryMatch<'a, 'a>,
        source: RopeSlice<'a>,
    ) -> (Option<InjectionLanguageMarker<'a>>, Option<Node<'a>>) {
        let mut injection_capture = None;
        let mut content_node = None;

        for capture in query_match.captures {
            let index = Some(capture.index);
            if index == self.injection_language_capture_index {
                let name = byte_range_to_str(capture.node.byte_range(), source);
                injection_capture = Some(InjectionLanguageMarker::Name(name));
            } else if index == self.injection_filename_capture_index {
                let name = byte_range_to_str(capture.node.byte_range(), source);
                let path = Path::new(name.as_ref()).to_path_buf();
                injection_capture = Some(InjectionLanguageMarker::Filename(path.into()));
            } else if index == self.injection_shebang_capture_index {
                let node_slice = source.byte_slice(capture.node.byte_range());

                // some languages allow space and newlines before the actual string content
                // so a shebang could be on either the first or second line
                // let lines = if let Ok(end) = node_slice.try_line_to_byte(2) {
                //     node_slice.byte_slice(..end)
                // } else {
                //     node_slice
                // };
                let lines = node_slice;

                static SHEBANG_REGEX: Lazy<regex_cursor::engines::meta::Regex> =
                    Lazy::new(|| regex_cursor::engines::meta::Regex::new(SHEBANG).unwrap());

                let input = regex_cursor::Input::new(RopeCursor::new(lines));
                injection_capture = SHEBANG_REGEX
                    .captures_iter(input)
                    .map(|cap| {
                        let cap = lines.byte_slice(cap.get_group(1).unwrap().range());
                        InjectionLanguageMarker::Shebang(cap.to_string())
                    })
                    .next()
            } else if index == self.injection_content_capture_index {
                content_node = Some(capture.node);
            }
        }
        (injection_capture, content_node)
    }

    fn injection_for_match<'a>(
        &self,
        query: &'a Query,
        query_match: &QueryMatch<'a, 'a>,
        source: RopeSlice<'a>,
    ) -> (
        Option<InjectionLanguageMarker<'a>>,
        Option<Node<'a>>,
        IncludedChildren,
    ) {
        let (mut injection_capture, content_node) = self.injection_pair(query_match, source);

        let mut included_children = IncludedChildren::default();
        for prop in query.property_settings(query_match.pattern_index) {
            match prop.key.as_ref() {
                // In addition to specifying the language name via the text of a
                // captured node, it can also be hard-coded via a `#set!` predicate
                // that sets the injection.language key.
                "injection.language" if injection_capture.is_none() => {
                    injection_capture = prop
                        .value
                        .as_ref()
                        .map(|s| InjectionLanguageMarker::Name(s.as_ref().into()));
                }

                // By default, injections do not include the *children* of an
                // `injection.content` node - only the ranges that belong to the
                // node itself. This can be changed using a `#set!` predicate that
                // sets the `injection.include-children` key.
                "injection.include-children" => included_children = IncludedChildren::All,

                // Some queries might only exclude named children but include unnamed
                // children in their `injection.content` node. This can be enabled using
                // a `#set!` predicate that sets the `injection.include-unnamed-children` key.
                "injection.include-unnamed-children" => {
                    included_children = IncludedChildren::Unnamed
                }
                _ => {}
            }
        }

        (injection_capture, content_node, included_children)
    }
}

#[derive(Clone)]
enum IncludedChildren {
    None,
    All,
    Unnamed,
}

impl Default for IncludedChildren {
    fn default() -> Self {
        Self::None
    }
}

// Compute the ranges that should be included when parsing an injection.
// This takes into account three things:
// * `parent_ranges` - The ranges must all fall within the *current* layer's ranges.
// * `nodes` - Every injection takes place within a set of nodes. The injection ranges
//   are the ranges of those nodes.
// * `includes_children` - For some injections, the content nodes' children should be
//   excluded from the nested document, so that only the content nodes' *own* content
//   is reparsed. For other injections, the content nodes' entire ranges should be
//   reparsed, including the ranges of their children.
fn intersect_ranges(
    parent_ranges: &[Range],
    nodes: &[Node],
    included_children: IncludedChildren,
) -> Vec<Range> {
    let mut cursor = nodes[0].walk();
    let mut result = Vec::new();
    let mut parent_range_iter = parent_ranges.iter();
    let mut parent_range = parent_range_iter
        .next()
        .expect("Layers should only be constructed with non-empty ranges vectors");
    for node in nodes.iter() {
        let mut preceding_range = Range {
            start_byte: 0,
            start_point: Point::new(0, 0),
            end_byte: node.start_byte(),
            end_point: node.start_position(),
        };
        let following_range = Range {
            start_byte: node.end_byte(),
            start_point: node.end_position(),
            end_byte: usize::MAX,
            end_point: Point::new(usize::MAX, usize::MAX),
        };

        for excluded_range in node
            .children(&mut cursor)
            .filter_map(|child| match included_children {
                IncludedChildren::None => Some(child.range()),
                IncludedChildren::All => None,
                IncludedChildren::Unnamed => {
                    if child.is_named() {
                        Some(child.range())
                    } else {
                        None
                    }
                }
            })
            .chain([following_range].iter().cloned())
        {
            let mut range = Range {
                start_byte: preceding_range.end_byte,
                start_point: preceding_range.end_point,
                end_byte: excluded_range.start_byte,
                end_point: excluded_range.start_point,
            };
            preceding_range = excluded_range;

            if range.end_byte < parent_range.start_byte {
                continue;
            }

            while parent_range.start_byte <= range.end_byte {
                if parent_range.end_byte > range.start_byte {
                    if range.start_byte < parent_range.start_byte {
                        range.start_byte = parent_range.start_byte;
                        range.start_point = parent_range.start_point;
                    }

                    if parent_range.end_byte < range.end_byte {
                        if range.start_byte < parent_range.end_byte {
                            result.push(Range {
                                start_byte: range.start_byte,
                                start_point: range.start_point,
                                end_byte: parent_range.end_byte,
                                end_point: parent_range.end_point,
                            });
                        }
                        range.start_byte = parent_range.end_byte;
                        range.start_point = parent_range.end_point;
                    } else {
                        if range.start_byte < range.end_byte {
                            result.push(range);
                        }
                        break;
                    }
                }

                if let Some(next_range) = parent_range_iter.next() {
                    parent_range = next_range;
                } else {
                    return result;
                }
            }
        }
    }
    result
}

// #[derive(Debug, Deserialize)]
// #[serde(rename_all = "kebab-case")]
// pub struct IndentationConfiguration {
//     pub tab_width: usize,
//     pub unit: String,
// }

// Adapter to convert rope chunks to bytes
pub struct ChunksBytes<'a> {
    chunks: crop::iter::Chunks<'a>,
}
impl<'a> Iterator for ChunksBytes<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        self.chunks.next().map(str::as_bytes)
    }
}

pub struct RopeProvider<'a>(pub RopeSlice<'a>);
impl<'a> TextProvider<&'a [u8]> for RopeProvider<'a> {
    type I = ChunksBytes<'a>;

    fn text(&mut self, node: Node) -> Self::I {
        let fragment = self.0.byte_slice(node.start_byte()..node.end_byte());
        ChunksBytes {
            chunks: fragment.chunks(),
        }
    }
}

// fn from_comment_tokens<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
// where
//     D: serde::Deserializer<'de>,
// {
//     #[derive(Deserialize)]
//     #[serde(untagged)]
//     enum CommentTokens {
//         Multiple(Vec<String>),
//         Single(String),
//     }
//     Ok(
//         Option::<CommentTokens>::deserialize(deserializer)?.map(|tokens| match tokens {
//             CommentTokens::Single(val) => vec![val],
//             CommentTokens::Multiple(vals) => vals,
//         }),
//     )
// }

// #[derive(Clone, Debug, Deserialize)]
// pub struct BlockCommentToken {
//     pub start: String,
//     pub end: String,
// }
//
// impl Default for BlockCommentToken {
//     fn default() -> Self {
//         BlockCommentToken {
//             start: "/*".to_string(),
//             end: "*/".to_string(),
//         }
//     }
// }

// fn from_block_comment_tokens<'de, D>(
//     deserializer: D,
// ) -> Result<Option<Vec<BlockCommentToken>>, D::Error>
// where
//     D: serde::Deserializer<'de>,
// {
//     #[derive(Deserialize)]
//     #[serde(untagged)]
//     enum BlockCommentTokens {
//         Multiple(Vec<BlockCommentToken>),
//         Single(BlockCommentToken),
//     }
//     Ok(
//         Option::<BlockCommentTokens>::deserialize(deserializer)?.map(|tokens| match tokens {
//             BlockCommentTokens::Single(val) => vec![val],
//             BlockCommentTokens::Multiple(vals) => vals,
//         }),
//     )
// }

#[derive(Debug, Clone)]
pub enum InjectionLanguageMarker<'a> {
    Name(Cow<'a, str>),
    Filename(Cow<'a, Path>),
    Shebang(String),
}

const SHEBANG: &str = r"#!\s*(?:\S*[/\\](?:env\s+(?:\-\S+\s+)*)?)?([^\s\.\d]+)";

fn read_query(language: &str, filename: &str) -> String {
    static INHERITS_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r";+\s*inherits\s*:?\s*([a-z_,()-]+)\s*").unwrap());

    let query = load_query(language, filename).unwrap_or_default();

    // replaces all "; inherits <language>(,<language>)*" with the queries of the given language(s)
    INHERITS_REGEX
        .replace_all(query, |captures: &regex::Captures| {
            captures[1]
                .split(',')
                .fold(String::new(), |mut output, language| {
                    // `write!` to a String cannot fail.
                    write!(output, "\n{}\n", read_query(language, filename)).unwrap();
                    output
                })
        })
        .to_string()
}

fn load_query(language: &str, filename: &str) -> Option<&'static str> {
    let file = QUERIES.get_file(format!("{}/{}", language, filename))?;
    file.contents_utf8()
}

// #[derive(Debug)]
// pub enum CapturedNode<'a> {
//     Single(Node<'a>),
//     /// Guaranteed to be not empty
//     Grouped(Vec<Node<'a>>),
// }
//
// impl<'a> CapturedNode<'a> {
//     pub fn start_byte(&self) -> usize {
//         match self {
//             Self::Single(n) => n.start_byte(),
//             Self::Grouped(ns) => ns[0].start_byte(),
//         }
//     }
//
//     pub fn end_byte(&self) -> usize {
//         match self {
//             Self::Single(n) => n.end_byte(),
//             Self::Grouped(ns) => ns.last().unwrap().end_byte(),
//         }
//     }
//
//     pub fn byte_range(&self) -> std::ops::Range<usize> {
//         self.start_byte()..self.end_byte()
//     }
// }

/// This is set to a constant in order to avoid performance problems for medium to large files. Set with `set_match_limit`.
/// Using such a limit means that we lose valid captures, so there is fundamentally a tradeoff here.
///
///
/// Old tree sitter versions used a limit of 32 by default until this limit was removed in version `0.19.5` (must now be set manually).
/// However, this causes performance issues for medium to large files.
/// In helix, this problem caused treesitter motions to take multiple seconds to complete in medium-sized rust files (3k loc).
///
///
/// Neovim also encountered this problem and reintroduced this limit after it was removed upstream
/// (see <https://github.com/neovim/neovim/issues/14897> and <https://github.com/neovim/neovim/pull/14915>).
/// The number used here is fundamentally a tradeoff between breaking some obscure edge cases and performance.
///
///
/// Neovim chose 64 for this value somewhat arbitrarily (<https://github.com/neovim/neovim/pull/18397>).
/// 64 is too low for some languages though. In particular, it breaks some highlighting for record fields in Erlang record definitions.
/// This number can be increased if new syntax highlight breakages are found, as long as the performance penalty is not too high.
const TREE_SITTER_MATCH_LIMIT: u32 = 256;

// #[derive(Debug)]
// pub struct TextObjectQuery {
//     pub query: Query,
// }

// impl TextObjectQuery {
//     /// Run the query on the given node and return sub nodes which match given
//     /// capture ("function.inside", "class.around", etc).
//     ///
//     /// Captures may contain multiple nodes by using quantifiers (+, *, etc),
//     /// and support for this is partial and could use improvement.
//     ///
//     /// ```query
//     /// (comment)+ @capture
//     ///
//     /// ; OR
//     /// (
//     ///   (comment)*
//     ///   .
//     ///   (function)
//     /// ) @capture
//     /// ```
//     pub fn capture_nodes<'a>(
//         &'a self,
//         capture_name: &str,
//         node: Node<'a>,
//         slice: RopeSlice<'a>,
//         cursor: &'a mut QueryCursor,
//     ) -> Option<impl Iterator<Item = CapturedNode<'a>>> {
//         self.capture_nodes_any(&[capture_name], node, slice, cursor)
//     }
//
//     /// Find the first capture that exists out of all given `capture_names`
//     /// and return sub nodes that match this capture.
//     pub fn capture_nodes_any<'a>(
//         &'a self,
//         capture_names: &[&str],
//         node: Node<'a>,
//         slice: RopeSlice<'a>,
//         cursor: &'a mut QueryCursor,
//     ) -> Option<impl Iterator<Item = CapturedNode<'a>>> {
//         let capture_idx = capture_names
//             .iter()
//             .find_map(|cap| self.query.capture_index_for_name(cap))?;
//
//         cursor.set_match_limit(TREE_SITTER_MATCH_LIMIT);
//
//         let nodes = cursor.captures(&self.query, node, RopeProvider(slice))
//             .filter_map(move |(mat, _)| {
//                 let nodes: Vec<_> = mat
//                     .captures
//                     .iter()
//                     .filter_map(|cap| (cap.index == capture_idx).then_some(cap.node))
//                     .collect();
//
//                 if nodes.len() > 1 {
//                     Some(CapturedNode::Grouped(nodes))
//                 } else {
//                     nodes.into_iter().map(CapturedNode::Single).next()
//                 }
//             });
//
//         Some(nodes)
//     }
// }
