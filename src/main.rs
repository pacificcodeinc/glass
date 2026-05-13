mod app;
mod config;
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

    let path = PathBuf::from(arg);
    let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());

    if canonical.is_dir() {
        Ok((canonical, None))
    } else {
        let parent = canonical
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        if !parent.is_dir() {
            std::fs::create_dir_all(&parent)
                .with_context(|| format!("failed to create directory: {}", parent.display()))?;
        }

        if !canonical.exists() {
            std::fs::File::create(&canonical)
                .with_context(|| format!("failed to create file: {}", canonical.display()))?;
        }

        Ok((parent, Some(canonical)))
    }
}
