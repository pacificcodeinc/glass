use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Write,
    Quit { force: bool },
    WriteQuit,
    Edit(PathBuf),
    Unknown(String),
}

pub fn parse_command(input: &str) -> Command {
    let trimmed = input.trim();

    match trimmed {
        "w" | "write" => Command::Write,
        "q" | "quit" => Command::Quit { force: false },
        "q!" | "quit!" => Command::Quit { force: true },
        "wq" | "x" => Command::WriteQuit,
        _ if trimmed.starts_with("e ") || trimmed.starts_with("edit ") => {
            let path = trimmed
                .split_once(' ')
                .map(|(_, path)| PathBuf::from(path.trim()))
                .unwrap_or_default();
            Command::Edit(path)
        }
        _ => Command::Unknown(trimmed.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_write_quit() {
        assert_eq!(parse_command("wq"), Command::WriteQuit);
        assert_eq!(parse_command("q!"), Command::Quit { force: true });
    }
}
