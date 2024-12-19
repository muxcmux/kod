use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::{
    fmt::Write,
    env,
    fs,
    time::SystemTime,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::channel,
};

fn out_dir() -> PathBuf {
    std::convert::Into::<PathBuf>::into(env::var("OUT_DIR").unwrap())
}

fn main() {
    let here = std::convert::Into::<PathBuf>::into(env::var("CARGO_MANIFEST_DIR").unwrap());

    println!("cargo::rustc-link-search=native={}", out_dir().display());

    println!("cargo::rerun-if-changed={}", here.join("src/language/config.json").display());

    fetch_grammars().expect("Failed to fetch tree-sitter grammars");

    grammar_codegen(
        &build_grammars().expect("Failed to build tree-sitter grammars")
    );
}

static CONFIG: &str = include_str!("src/language/config.json");

fn get_grammar_config() -> Vec<GrammarConfiguration> {
    serde_json::from_str::<Configuration>(CONFIG)
        .expect("Cannot parse language config.json")
        .grammars
}

#[derive(Debug, Deserialize)]
struct Configuration {
    grammars: Vec<GrammarConfiguration>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrammarConfiguration {
    #[serde(rename = "name")]
    grammar_id: String,
    source: GrammarSource,
}

impl GrammarConfiguration {
    fn lib_name(&self) -> String {
        format!("tree-sitter-{}", self.grammar_id)
    }

    fn lib_file_name(&self) -> String {
        format!("libtree-sitter-{}.a", self.grammar_id)
    }

    fn ts_language_fn_name(&self) -> String {
        self.lib_name().replace("-", "_")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase", untagged)]
enum GrammarSource {
    Local {
        path: String,
    },
    Git {
        #[serde(rename = "git")]
        remote: String,
        #[serde(rename = "rev")]
        revision: String,
        subpath: Option<String>,
    },
}

const REMOTE_NAME: &str = "origin";

fn fetch_grammars() -> Result<()> {
    // We do not need to fetch local grammars.
    let mut grammars = get_grammar_config();
    grammars.retain(|grammar| !matches!(grammar.source, GrammarSource::Local { .. }));

    println!("Fetching {} grammars", grammars.len());
    let results = run_parallel(grammars, fetch_grammar);

    let mut errors = Vec::new();
    let mut git_updated = Vec::new();
    let mut git_up_to_date = 0;
    let mut non_git = Vec::new();

    for (grammar_id, res) in results {
        match res {
            Ok(FetchStatus::GitUpToDate) => git_up_to_date += 1,
            Ok(FetchStatus::GitUpdated { revision }) => git_updated.push((grammar_id, revision)),
            Ok(FetchStatus::NonGit) => non_git.push(grammar_id),
            Err(e) => errors.push((grammar_id, e)),
        }
    }

    non_git.sort_unstable();
    git_updated.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    if git_up_to_date != 0 {
        println!("{} up to date git grammars", git_up_to_date);
    }

    if !non_git.is_empty() {
        println!("{} non git grammars", non_git.len());
        println!("\t{:?}", non_git);
    }

    if !git_updated.is_empty() {
        println!("{} updated grammars", git_updated.len());
        // We checked the vec is not empty, unwrapping will not panic
        let longest_id = git_updated.iter().map(|x| x.0.len()).max().unwrap();
        for (id, rev) in git_updated {
            println!(
                "\t{id:width$} now on {rev}",
                id = id,
                width = longest_id,
                rev = rev
            );
        }
    }

    if !errors.is_empty() {
        let len = errors.len();
        for (i, (grammar, error)) in errors.into_iter().enumerate() {
            println!("Failure {}/{len}: {grammar} {error}", i + 1);
        }
        bail!("{len} grammars failed to fetch");
    }

    Ok(())
}

fn build_grammars() -> Result<Vec<GrammarConfiguration>> {
    let grammars = get_grammar_config();
    println!("Building {} grammars", grammars.len());
    let results = run_parallel(grammars, move |grammar| {
        build_grammar(grammar)
    });

    let mut errors = Vec::new();
    let mut already_built = 0;
    let mut built = Vec::new();
    let mut grammars = Vec::new();

    for (grammar_id, res) in results {
        match res {
            Ok(BuildStatus::AlreadyBuilt(grammar)) => {
                // cc::Build takes care of emitting cargo metadata
                // but for already built grammars, we need to tell
                // it to link to the already built lib
                println!("cargo::rustc-link-lib=static={}", grammar.lib_name());
                grammars.push(grammar);
                already_built += 1
            },
            Ok(BuildStatus::Built(grammar)) => {
                grammars.push(grammar);
                built.push(grammar_id)
            }
            Err(e) => errors.push((grammar_id, e)),
        }
    }

    built.sort_unstable();

    if already_built != 0 {
        println!("{} grammars already built", already_built);
    }

    if !built.is_empty() {
        println!("{} grammars built now", built.len());
        println!("\t{:?}", built);
    }

    if !errors.is_empty() {
        let len = errors.len();
        for (i, (grammar_id, error)) in errors.into_iter().enumerate() {
            println!("Failure {}/{len}: {grammar_id} {error}", i + 1);
        }
        bail!("{len} grammars failed to build");
    }

    Ok(grammars)
}

fn run_parallel<F, Res>(grammars: Vec<GrammarConfiguration>, job: F) -> Vec<(String, Result<Res>)>
where
    F: Fn(GrammarConfiguration) -> Result<Res> + Send + 'static + Clone,
    Res: Send + 'static,
{
    let pool = threadpool::Builder::new().build();
    let (tx, rx) = channel();

    for grammar in grammars {
        let tx = tx.clone();
        let job = job.clone();

        pool.execute(move || {
            // Ignore any SendErrors, if any job in another thread has encountered an
            // error the Receiver will be closed causing this send to fail.
            let _ = tx.send((grammar.grammar_id.clone(), job(grammar)));
        });
    }

    drop(tx);

    rx.iter().collect()
}

enum FetchStatus {
    GitUpToDate,
    GitUpdated { revision: String },
    NonGit,
}

fn fetch_grammar(grammar: GrammarConfiguration) -> Result<FetchStatus> {
    if let GrammarSource::Git {
        remote, revision, ..
    } = grammar.source
    {
        let grammar_dir = std::convert::Into::<PathBuf>::into(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("grammars")
            .join(&grammar.grammar_id);

        fs::create_dir_all(&grammar_dir).context(format!(
            "Could not create grammar directory {:?}",
            grammar_dir
        ))?;

        // create the grammar dir contains a git directory
        if !grammar_dir.join(".git").exists() {
            git(&grammar_dir, ["init"])?;
        }

        // ensure the remote matches the configured remote
        if get_remote_url(&grammar_dir).map_or(true, |s| s != remote) {
            set_remote(&grammar_dir, &remote)?;
        }

        // ensure the revision matches the configured revision
        if get_revision(&grammar_dir).map_or(true, |s| s != revision) {
            // Fetch the exact revision from the remote.
            // Supported by server-side git since v2.5.0 (July 2015),
            // enabled by default on major git hosts.
            git(
                &grammar_dir,
                ["fetch", "--depth", "1", REMOTE_NAME, &revision],
            )?;
            git(&grammar_dir, ["checkout", &revision])?;

            Ok(FetchStatus::GitUpdated { revision })
        } else {
            Ok(FetchStatus::GitUpToDate)
        }
    } else {
        Ok(FetchStatus::NonGit)
    }
}

// Sets the remote for a repository to the given URL, creating the remote if
// it does not yet exist.
fn set_remote(repository_dir: &Path, remote_url: &str) -> Result<String> {
    git(
        repository_dir,
        ["remote", "set-url", REMOTE_NAME, remote_url],
    )
    .or_else(|_| git(repository_dir, ["remote", "add", REMOTE_NAME, remote_url]))
}

fn get_remote_url(repository_dir: &Path) -> Option<String> {
    git(repository_dir, ["remote", "get-url", REMOTE_NAME]).ok()
}

fn get_revision(repository_dir: &Path) -> Option<String> {
    git(repository_dir, ["rev-parse", "HEAD"]).ok()
}

// A wrapper around 'git' commands which returns stdout in success and a
// helpful error message showing the command, stdout, and stderr in error.
fn git<I, S>(repository_dir: &Path, args: I) -> Result<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new("git");
    command.args(args).current_dir(repository_dir);
    println!("Running `{:?}`", command);
    let output = command.output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned())
    } else {
        // TODO: figure out how to display the git command using `args`
        Err(anyhow!(
            "Git command failed.\nStdout: {}\nStderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ))
    }
}

enum BuildStatus {
    AlreadyBuilt(GrammarConfiguration),
    Built(GrammarConfiguration),
}

fn build_grammar(grammar: GrammarConfiguration) -> Result<BuildStatus> {
    let grammar_dir = if let GrammarSource::Local { path } = &grammar.source {
        PathBuf::from(&path)
    } else {
        std::convert::Into::<PathBuf>::into(env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("grammars")
            .join(&grammar.grammar_id)
    };

    let grammar_dir_entries = grammar_dir.read_dir().with_context(|| {
        format!(
            "Failed to read directory {:?}",
            grammar_dir
        )
    })?;

    if grammar_dir_entries.count() == 0 {
        return Err(anyhow!(
            "Directory {:?} is empty.",
            grammar_dir
        ));
    };

    let path = match &grammar.source {
        GrammarSource::Git {
            subpath: Some(subpath),
            ..
        } => grammar_dir.join(subpath),
        _ => grammar_dir,
    }
    .join("src");

    build_tree_sitter_library(&path, grammar)
}

fn build_tree_sitter_library(src_path: &Path, grammar: GrammarConfiguration) -> Result<BuildStatus> {
    let out_dir = out_dir();

    let parser_path = src_path.join("parser.c");
    let mut scanner_path = src_path.join("scanner.c");

    let mut build = cc::Build::new();
    build
        .include(src_path)
        .opt_level(3)
        .cargo_warnings(false)
        .file(&parser_path);

    let scanner_path = if scanner_path.exists() {
        build.file(&scanner_path);
        Some(scanner_path)
    } else {
        scanner_path.set_extension("cc");
        if scanner_path.exists() {
            build
                .cpp(true)
                .file(&scanner_path);
            Some(scanner_path)
        } else {
            None
        }
    };

    let static_library_path = out_dir
        .join(grammar.lib_file_name());

    if let Some(scanner_path) = scanner_path.as_ref().and_then(|path| path.to_str()) {
        println!("cargo::rerun-if-changed={scanner_path}");
    }

    if let Some(parser_path) = parser_path.to_str() {
        println!("cargo::rerun-if-changed={parser_path}");
    }

    let recompile = needs_recompile(&static_library_path, &parser_path, scanner_path.as_ref())
        .context("Failed to compare source and binary timestamps")?;

    if !recompile {
        return Ok(BuildStatus::AlreadyBuilt(grammar));
    }

    build.compile(&grammar.lib_name());

    Ok(BuildStatus::Built(grammar))
}

fn needs_recompile(
    lib_path: &Path,
    parser_c_path: &Path,
    scanner_path: Option<&PathBuf>,
) -> Result<bool> {
    if !lib_path.exists() {
        return Ok(true);
    }
    let lib_mtime = mtime(lib_path)?;
    if mtime(parser_c_path)? > lib_mtime {
        return Ok(true);
    }
    if let Some(scanner_path) = scanner_path {
        if mtime(scanner_path)? > lib_mtime {
            return Ok(true);
        }
    }
    Ok(false)
}

fn mtime(path: &Path) -> Result<SystemTime> {
    Ok(fs::metadata(path)?.modified()?)
}

fn grammar_codegen(grammars: &[GrammarConfiguration]){
    let dest_path = out_dir().join("grammars.rs");

    let externs = grammars.iter()
        .fold(String::new(), |mut s, g| {
            writeln!(s, "extern \"C\" {{ fn {}() -> tree_sitter::Language; }}", g.ts_language_fn_name()).unwrap();
            s
        });

    let map = grammars.iter()
        .fold(String::new(), |mut s, g| {
            writeln!(s, "\"{}\" => Some(unsafe {{ {}() }}),", g.grammar_id, g.ts_language_fn_name()).unwrap();
            s
        });

    let get_language = format!("pub fn get_language(name: &str) -> Option<tree_sitter::Language> {{
        match name {{
            {}
            _ => {{
                log::info!(\"Tree-sitter grammar `{{name}}` was not found\");
                None
            }},
        }}
    }}
    ", map);

    fs::write(
        &dest_path,
        [
            externs,
            get_language,
        ].into_iter().fold(String::new(), |mut s, item| {
            writeln!(s, "{}", item).unwrap();
            s
        })
    ).unwrap();
}
