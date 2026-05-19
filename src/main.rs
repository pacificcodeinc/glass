mod app;
mod config;
mod document;
mod editor;
mod fs;
mod markdown;
mod terminal;
mod ui;

use std::{env, path::PathBuf};

use anyhow::{Context, Result, bail};

use crate::{app::App, terminal::TerminalSession};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> Result<()> {
    let (notes_dir, initial_file) = parse_args()?;
    let app = App::new(notes_dir, initial_file)?;
    TerminalSession::run(app)
}

fn parse_args() -> Result<(PathBuf, Option<PathBuf>)> {
    let mut args = env::args_os();
    let program = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .unwrap_or_else(|| "glass".to_string());

    let Some(arg) = args.next() else {
        bail!("usage: {program} <path>");
    };

    if args.next().is_some() {
        bail!("usage: {program} <path>");
    }

    let arg_str = arg.to_string_lossy();
    if arg_str == "--version" || arg_str == "-v" {
        println!("glass {VERSION}");
        std::process::exit(0);
    }

    parse_path_arg(PathBuf::from(arg))
}

fn parse_path_arg(path: PathBuf) -> Result<(PathBuf, Option<PathBuf>)> {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

    if canonical.is_dir() {
        Ok((canonical, None))
    } else {
        let parent = canonical
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or(env::current_dir().context("failed to resolve current directory")?);

        Ok((parent, Some(canonical)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_arg_does_not_create_file() -> Result<()> {
        let dir = env::temp_dir().join(format!("glass-arg-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let file = dir.join("new-note.md");

        let (_notes_dir, initial_file) = parse_path_arg(file.clone())?;

        assert_eq!(initial_file.as_deref(), Some(file.as_path()));
        assert!(!file.exists());

        std::fs::remove_dir(dir)?;
        Ok(())
    }
}
