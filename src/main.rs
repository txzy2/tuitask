#![warn(clippy::all, clippy::pedantic)]
use directories::ProjectDirs;
use std::fs;
use std::path::PathBuf;
use tokio::runtime::Runtime;
use tuitask::{app::App, logger};

fn default_log_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("", "", "tuitask") {
        let cache_dir = proj_dirs.cache_dir();
        let _ = fs::create_dir_all(cache_dir);
        cache_dir.join("app.log")
    } else {
        PathBuf::from("logs/app.log")
    }
}

fn main() -> color_eyre::Result<()> {
    logger::init(default_log_path())?;
    logger::info("Application started")?;

    color_eyre::install()?;
    dotenvy::dotenv().ok();

    let runtime = Runtime::new()?;
    let handle = runtime.handle().clone();

    let terminal = ratatui::init();
    let result = App::new(handle).run(terminal);

    if let Err(ref error) = result {
        let _ = logger::error(format!("Application error: {error}"));
    } else {
        let _ = logger::info("Application exited successfully");
    }
    runtime.shutdown_background();
    ratatui::restore();

    result
}
