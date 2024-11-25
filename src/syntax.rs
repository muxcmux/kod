pub struct LanguageConfiguration {
    pub id: String, // c-sharp, rust, tsx
    // see the table under https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#textDocumentItem
    pub language_server_language_id: Option<String>, // csharp, rust, typescriptreact, for the language-server
    pub scope: String, // source.rust
    pub file_types: Vec<FileType>, // filename extension or ends_with? <Gemfile, rb, etc>
    pub shebangs: Vec<String>, // interpreter(s) associated with language
    pub roots: Vec<String>, // these indicate project roots <.git, Cargo.toml>
    pub comment_tokens: Option<Vec<String>>,
    pub block_comment_tokens: Option<Vec<(String, String)>>,
    pub text_width: Option<usize>,

    pub grammar: Option<String>, // tree-sitter grammar name, defaults to language_id

    // content_regex
    pub injection_regex: Option<Regex>,
    pub(crate) highlight_config: OnceCell<Option<Arc<HighlightConfiguration>>>,
    // tags_config OnceCell<> https://github.com/tree-sitter/tree-sitter/pull/583
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        serialize_with = "serialize_lang_features",
        deserialize_with = "deserialize_lang_features"
    )]
    pub language_servers: Vec<LanguageServerFeatures>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent: Option<IndentationConfiguration>,

    #[serde(skip)]
    pub(crate) indent_query: OnceCell<Option<Query>>,
    #[serde(skip)]
    pub(crate) textobject_query: OnceCell<Option<TextObjectQuery>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debugger: Option<DebugAdapterConfig>,

    /// Automatic insertion of pairs to parentheses, brackets,
    /// etc. Defaults to true. Optionally, this can be a list of 2-tuples
    /// to specify a list of characters to pair. This overrides the
    /// global setting.
    #[serde(default, skip_serializing, deserialize_with = "deserialize_auto_pairs")]
    pub auto_pairs: Option<AutoPairs>,

    pub rulers: Option<Vec<u16>>, // if set, override editor's rulers

    /// Hardcoded LSP root directories relative to the workspace root, like `examples` or `tools/fuzz`.
    /// Falling back to the current working directory if none are configured.
    pub workspace_lsp_roots: Option<Vec<PathBuf>>,
    #[serde(default)]
    pub persistent_diagnostic_sources: Vec<String>,
}

pub struct HighlightConfiguration {
    pub language: Grammar,
    pub query: Query,
    injections_query: Query,
    combined_injections_patterns: Vec<usize>,
    highlights_pattern_index: usize,
    highlight_indices: ArcSwap<Vec<Option<Highlight>>>,
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

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum FileType {
    /// The extension of the file, either the `Path::extension` or the full
    /// filename if the file does not have an extension.
    Extension(String),
    /// A Unix-style path glob. This is compared to the file's absolute path, so
    /// it can be used to detect files based on their directories. If the glob
    /// is not an absolute path and does not already start with a glob pattern,
    /// a glob pattern will be prepended to it.
    Glob(globset::Glob),
}
