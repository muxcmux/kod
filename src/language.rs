pub(crate) mod syntax;
// pub(crate) mod tree_cursor;
pub(crate) mod grammar;

use std::{borrow::Cow, collections::HashMap, path::Path, sync::Arc};

use crop::RopeSlice;
use crossterm::style::Color;
use globset::{Glob, GlobSet, GlobSetBuilder};
use grammar::get_language;
use once_cell::sync::{Lazy, OnceCell};
use regex::Regex;
use serde::Deserialize;
use syntax::{read_query, HighlightConfiguration, InjectionLanguageMarker, SHEBANG};

use crate::ui::theme::color;

pub static LANG_CONFIG: Lazy<Loader> = Lazy::new(|| {
    let config = serde_json::from_str(include_str!("language/config.json"))
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

fn deserialize_color<'de, D>(deserializer: D) -> Result<Option<Color>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)?
        .map(|buf| color(&buf).map_err(serde::de::Error::custom))
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
    #[serde(default, deserialize_with = "deserialize_color")]
    pub color: Option<Color>,

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
