mod app;
mod config;
mod debug_render;
mod editor;
mod fs;
mod markdown;
mod terminal;
mod ui;

use std::{
    env,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

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
            let output = render_path_to_ansi(notes_dir, initial_file, width, height)?;
            write_stdout(&output)
        }
        LaunchMode::Help { program } => write_stdout(&help_text(&program)),
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
    Help {
        program: String,
    },
}

fn parse_args() -> Result<LaunchMode> {
    parse_args_os(env::args_os().collect(), std::io::stdout().is_terminal())
}

fn parse_args_os(args: Vec<std::ffi::OsString>, stdout_is_terminal: bool) -> Result<LaunchMode> {
    let mut args = args.into_iter();
    let program = args
        .next()
        .and_then(display_program_name)
        .unwrap_or_else(|| "glass".to_string());

    let Some(arg) = args.next() else {
        return Ok(LaunchMode::Help { program });
    };

    let arg_str = arg.to_string_lossy();
    let rest = args.collect::<Vec<_>>();

    if arg_str == "--help" || arg_str == "-h" {
        if !rest.is_empty() {
            bail!("usage: {program} --help");
        }
        return Ok(LaunchMode::Help { program });
    }

    if arg_str == "--version" || arg_str == "-v" {
        println!("glass {VERSION}");
        std::process::exit(0);
    }

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
    if stdout_is_terminal {
        Ok(LaunchMode::App {
            notes_dir,
            initial_file,
        })
    } else {
        Ok(LaunchMode::Render {
            notes_dir,
            initial_file,
            width: DEFAULT_RENDER_WIDTH,
            height: None,
        })
    }
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

fn display_program_name(arg: std::ffi::OsString) -> Option<String> {
    let raw = arg.into_string().ok()?;
    Path::new(&raw)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .or(Some(raw))
}

fn write_stdout(output: &str) -> Result<()> {
    let mut stdout = io::stdout().lock();
    match stdout.write_all(output.as_bytes()) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error).context("failed to write output"),
    }
}

fn help_text(program: &str) -> String {
    format!(
        "\
Glass {VERSION}

A fast terminal Markdown editor with Vim-inspired movement, live Markdown
rendering, link navigation, command/search sheets, and ANSI render debugging.

USAGE:
  {program} <path>
  {program} --render [--width <columns>] [--height <rows>] <path>
  {program} --help
  {program} --version

ARGUMENTS:
  <path>
      A notes directory or Markdown file.

      When <path> is a directory, Glass opens the first note from that
      directory's file tree when one is available. When <path> is a file,
      Glass opens that file and uses its parent directory for file navigation.
      Missing files are allowed and are created only when saved.

OPTIONS:
  -h, --help
      Print this help text and exit.

  -v, --version
      Print the Glass version and exit.

  --render
      Render a non-interactive ANSI snapshot of <path> to stdout. The snapshot
      uses the same Ratatui UI renderer as the editor, includes color/style
      escape codes, renders the document body, and ends with the status bar.
      This is intended for debugging visual regressions directly in a terminal.

  --dump-render
      Alias for --render.

  --width <columns>
      Width for --render output. Defaults to {DEFAULT_RENDER_WIDTH}.

  --height <rows>
      Optional height for --render output. When omitted, --render renders the
      full document height plus the status bar. When provided, the document body
      is clipped to fit and the status bar is still rendered.

OUTPUT:
  When stdout is a terminal, {program} <path> opens the interactive editor.
  When stdout is redirected or piped, {program} <path> automatically writes the
  same ANSI render snapshot as --render. Use a terminal that preserves ANSI
  color, or a pager such as less -R, when inspecting piped render output.

INTERACTIVE MODES:
  Normal
      Navigate, select, follow links, open command/search sheets, and run
      editing commands.

  Insert
      Edit text directly. Esc returns to Normal mode.

  Visual
      Select whole lines for line-oriented edits.

  Command line
      Type ':' for commands or '/' for search. The command sheet provides fuzzy
      completion for commands, files, and search results where relevant.

NORMAL MODE KEYS:
  i              enter Insert mode
  a              append after cursor
  v              enter Visual mode
  :              open command line
  /              search the current file
  h j k l        move left/down/up/right
  w b            move word forward/backward
  0 ^ $          line start, first non-blank, line end
  gg G           document top/bottom
  n N            next/previous search result
  dd             delete current line
  d + motion     delete by motion
  x              delete character under cursor
  u              undo last edit
  Enter          toggle checkbox or follow link under cursor
  gf             follow link under cursor
  Ctrl+C         quit

INSERT MODE KEYS:
  Esc            return to Normal mode
  Tab            insert four spaces
  Backspace      delete previous character
  Option-Left    move one word left
  Option-Right   move one word right
  Cmd-Left       move to line start
  Cmd-Right      move to line end
  Cmd-Delete     delete to line start
  Cmd-ForwardDel delete to line end

COMMANDS:
  :w, :write
      Save the current file.

  :q, :quit
      Quit when there are no unsaved changes.

  :q!, :quit!
      Quit and discard unsaved changes.

  :wq, :x
      Save and quit.

  :e <path>, :edit <path>
      Open a file path relative to the notes directory, or create it on save if
      it does not exist yet.

MOUSE:
  Click          move the cursor
  Drag           select text and copy it immediately
  Wheel          scroll through wrapped visual rows
  Cmd-click      open the link under the pointer

EXAMPLES:
  {program} .
  {program} README.md
  {program} README.md | less -R
  {program} README.md > render.ansi
  {program} --render README.md
  {program} --render --width 90 benchmark.md
  {program} --render --width 90 --height 24 benchmark.md
"
    )
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

    #[test]
    fn help_args_parse_without_path() -> Result<()> {
        assert_eq!(
            parse_args_from(["glass"])?,
            LaunchMode::Help {
                program: "glass".to_string()
            }
        );
        assert_eq!(
            parse_args_from(["glass", "--help"])?,
            LaunchMode::Help {
                program: "glass".to_string()
            }
        );
        Ok(())
    }

    #[test]
    fn path_args_auto_render_when_stdout_is_not_terminal() -> Result<()> {
        let mode = parse_args_from_with_stdout(["glass", "README.md"], false)?;

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
    fn path_args_open_app_when_stdout_is_terminal() -> Result<()> {
        let mode = parse_args_from_with_stdout(["glass", "README.md"], true)?;

        assert!(matches!(mode, LaunchMode::App { .. }));
        Ok(())
    }

    #[test]
    fn help_text_documents_render_and_commands() {
        let help = help_text("glass");

        assert!(help.contains("USAGE:"));
        assert!(help.contains("glass --render [--width <columns>] [--height <rows>] <path>"));
        assert!(help.contains("ends with the status bar"));
        assert!(help.contains("stdout is redirected or piped"));
        assert!(help.contains(":e <path>, :edit <path>"));
        assert!(help.contains("Cmd-click"));
    }

    #[test]
    fn displayed_program_name_uses_executable_name() {
        assert_eq!(
            display_program_name(std::ffi::OsString::from("target/debug/glass")).as_deref(),
            Some("glass")
        );
    }

    fn parse_args_from<const N: usize>(args: [&str; N]) -> Result<LaunchMode> {
        parse_args_from_with_stdout(args, true)
    }

    fn parse_args_from_with_stdout<const N: usize>(
        args: [&str; N],
        stdout_is_terminal: bool,
    ) -> Result<LaunchMode> {
        parse_args_os(
            args.iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>(),
            stdout_is_terminal,
        )
    }
}
