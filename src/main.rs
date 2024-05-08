use kod::application::Application;
use anyhow::Result;

fn setup_logging() -> Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| out.finish(format_args!("{}: {}", record.level(), message)))
        .level(log::LevelFilter::Debug)
        .chain(fern::log_file("log.log")?)
        .apply()?;

    Ok(())
}

fn main() -> Result<()> {
    setup_logging()?;

    let mut app = Application::new()?;

    app.run();

    Ok(())
}
