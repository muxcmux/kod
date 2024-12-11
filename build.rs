use std::fmt::Write;
use std::{env, fs, path::{Path, PathBuf}};

struct Grammar {
    path: PathBuf,
    name: String,
}

impl Grammar {
    fn function_name(&self) -> String {
        self.name.replace("-", "_")
    }
}

impl TryFrom<fs::DirEntry> for Grammar {
    type Error = String;

    fn try_from(value: fs::DirEntry) -> Result<Self, Self::Error> {
        let name = value.path().iter().last().unwrap().to_string_lossy().to_string();
        if !name.starts_with("tree-sitter-") {
            return Err("Dir name does not start with `tree-sitter-`".into());
        }
        Ok(Self { name, path: value.path() })
    }
}

fn main() {
    let here = env::var("CARGO_MANIFEST_DIR").unwrap();
    let path = format!("{here}/grammars");
    let grammars: Vec<Grammar> = fs::read_dir(path)
        .unwrap()
        .filter_map(|f| f.unwrap().try_into().ok())
        .collect();

    link_grammars(&grammars);
    grammar_codegen(&grammars);
}

fn link_grammars(grammars: &[Grammar]) {
    for grammar in grammars {
        println!("cargo::rustc-link-search={}", grammar.path.display());
        println!("cargo::rustc-link-lib=static={}", grammar.name);
    }
}

fn grammar_codegen(grammars: &[Grammar]){
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("grammars.rs");

    let externs = grammars.iter()
        .fold(String::new(), |mut s, g| {
            writeln!(s, "extern \"C\" {{ fn {}() -> tree_sitter::Language; }}", g.function_name()).unwrap();
            s
        });

    let map = grammars.iter()
        .fold(String::new(), |mut s, g| {
            writeln!(s, "\"{}\" => Some(unsafe {{ {}() }}),", g.name, g.function_name()).unwrap();
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
