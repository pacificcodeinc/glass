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

fn main() -> Result<()> {
    let notes_dir = parse_notes_dir()?;
    let app = App::new(notes_dir)?;
    TerminalSession::run(app)
}

fn parse_notes_dir() -> Result<PathBuf> {
    let mut args = env::args_os();
    let program = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .unwrap_or_else(|| "glassnotes".to_string());

    let Some(notes_dir) = args.next() else {
        bail!("usage: {program} <notes-dir>");
    };

    if args.next().is_some() {
        bail!("usage: {program} <notes-dir>");
    }

    let path = PathBuf::from(notes_dir);
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to open notes directory: {}", path.display()))?;

    if !canonical.is_dir() {
        bail!("notes path is not a directory: {}", canonical.display());
    }

    Ok(canonical)
}
