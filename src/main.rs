mod app;
mod config;
mod debug_render;
mod editor;
mod fs;
mod markdown;
mod terminal;
mod ui;

use std::{env, path::PathBuf};

use anyhow::{Context, Result, bail};

use crate::{app::App, debug_render::render_path_to_ansi, terminal::TerminalSession};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_RENDER_WIDTH: u16 = 100;

fn main() -> Result<()> {
    match parse_args()? {
        LaunchMode::App {
            notes_dir,
            initial_file,
        } => {
            let app = App::new(notes_dir, initial_file)?;
            TerminalSession::run(app)
        }
        LaunchMode::Render {
            notes_dir,
            initial_file,
            width,
            height,
        } => {
            print!(
                "{}",
                render_path_to_ansi(notes_dir, initial_file, width, height)?
            );
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchMode {
    App {
        notes_dir: PathBuf,
        initial_file: Option<PathBuf>,
    },
    Render {
        notes_dir: PathBuf,
        initial_file: Option<PathBuf>,
        width: u16,
        height: Option<u16>,
    },
}

fn parse_args() -> Result<LaunchMode> {
    parse_args_os(env::args_os().collect())
}

fn parse_args_os(args: Vec<std::ffi::OsString>) -> Result<LaunchMode> {
    let mut args = args.into_iter();
    let program = args
        .next()
        .and_then(|arg| arg.into_string().ok())
        .unwrap_or_else(|| "glass".to_string());

    let Some(arg) = args.next() else {
        bail!("usage: {program} <path>");
    };

    let arg_str = arg.to_string_lossy();
    if arg_str == "--version" || arg_str == "-v" {
        println!("glass {VERSION}");
        std::process::exit(0);
    }

    let rest = args.collect::<Vec<_>>();
    if arg_str == "--render" || arg_str == "--dump-render" {
        let mut width = DEFAULT_RENDER_WIDTH;
        let mut height = None;
        let mut path = None;
        let mut index = 0usize;
        while index < rest.len() {
            let value = rest[index].to_string_lossy();
            match value.as_ref() {
                "--width" => {
                    index += 1;
                    width = parse_u16_arg(rest.get(index), "--width")?;
                }
                "--height" => {
                    index += 1;
                    height = Some(parse_u16_arg(rest.get(index), "--height")?);
                }
                _ if path.is_none() => path = Some(PathBuf::from(&rest[index])),
                _ => bail!("usage: {program} --render [--width n] [--height n] <path>"),
            }
            index += 1;
        }
        let Some(path) = path else {
            bail!("usage: {program} --render [--width n] [--height n] <path>");
        };
        let (notes_dir, initial_file) = parse_path_arg(path)?;
        return Ok(LaunchMode::Render {
            notes_dir,
            initial_file,
            width,
            height,
        });
    }

    if !rest.is_empty() {
        bail!("usage: {program} <path>");
    }

    let (notes_dir, initial_file) = parse_path_arg(PathBuf::from(arg))?;
    Ok(LaunchMode::App {
        notes_dir,
        initial_file,
    })
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

fn parse_u16_arg(value: Option<&std::ffi::OsString>, label: &str) -> Result<u16> {
    let Some(value) = value else {
        bail!("missing value for {label}");
    };
    let parsed = value
        .to_string_lossy()
        .parse::<u16>()
        .with_context(|| format!("invalid value for {label}"))?;
    Ok(parsed.max(1))
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

    #[test]
    fn render_args_default_to_full_height() -> Result<()> {
        let mode = parse_args_from(["glass", "--render", "README.md"])?;

        assert!(matches!(
            mode,
            LaunchMode::Render {
                width: DEFAULT_RENDER_WIDTH,
                height: None,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn render_args_parse_optional_dimensions() -> Result<()> {
        let mode = parse_args_from([
            "glass",
            "--render",
            "--width",
            "80",
            "--height",
            "24",
            "README.md",
        ])?;

        assert!(matches!(
            mode,
            LaunchMode::Render {
                width: 80,
                height: Some(24),
                ..
            }
        ));
        Ok(())
    }

    fn parse_args_from<const N: usize>(args: [&str; N]) -> Result<LaunchMode> {
        parse_args_os(
            args.iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>(),
        )
    }
}
