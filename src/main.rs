use kod::application::Application;
use anyhow::Result;
use std::{env, fs, path::PathBuf};

fn kod_dir() -> PathBuf {
    let home = env::var("HOME").expect("Can't find home dir");
    let kod_dir = PathBuf::from(format!("{home}/.local/share/kod"));

    if !kod_dir.exists() {
        fs::create_dir_all(&kod_dir).expect("Can't create kod dir: ~/.local/share/kod");
    }

    kod_dir
}

fn setup_logging() -> Result<()> {
    let mut kod_dir = kod_dir();
    kod_dir.push("log.log");

    fern::Dispatch::new()
        .format(|out, message, record| out.finish(format_args!("{}: {}", record.level(), message)))
        .level(log::LevelFilter::Debug)
        .chain(fern::log_file(&kod_dir)?)
        .apply()?;

    Ok(())
}

fn main() -> Result<()> {
    setup_logging()?;

    let mut app = Application::default();

    app.run()?;

    Ok(())
}
