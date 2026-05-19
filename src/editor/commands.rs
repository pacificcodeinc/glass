use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Write,
    Quit { force: bool },
    WriteQuit,
    Edit(PathBuf),
    Table { rows: usize, columns: usize },
    TableRow { placement: TableRowPlacement },
    TableColumn { placement: TableColumnPlacement },
    Unknown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableRowPlacement {
    Above,
    Below,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableColumnPlacement {
    Left,
    Right,
}

pub fn parse_command(input: &str) -> Command {
    let trimmed = input.trim();

    match trimmed {
        "w" | "write" => Command::Write,
        "q" | "quit" => Command::Quit { force: false },
        "q!" | "quit!" => Command::Quit { force: true },
        "wq" | "x" => Command::WriteQuit,
        "table" => Command::Table {
            rows: 2,
            columns: 2,
        },
        "row" | "table row" | "row below" | "table row below" => Command::TableRow {
            placement: TableRowPlacement::Below,
        },
        "row above" | "table row above" => Command::TableRow {
            placement: TableRowPlacement::Above,
        },
        "col" | "column" | "table column" | "col right" | "column right" | "table column right" => {
            Command::TableColumn {
                placement: TableColumnPlacement::Right,
            }
        }
        "col left" | "column left" | "table column left" => Command::TableColumn {
            placement: TableColumnPlacement::Left,
        },
        _ if trimmed.starts_with("table ") => {
            let spec = trimmed
                .split_once(' ')
                .map(|(_, value)| value.trim())
                .unwrap_or_default();
            parse_table_size(spec)
                .map(|(rows, columns)| Command::Table { rows, columns })
                .unwrap_or_else(|| Command::Unknown(trimmed.to_string()))
        }
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

fn parse_table_size(spec: &str) -> Option<(usize, usize)> {
    let (rows, columns) = spec.split_once('x').or_else(|| spec.split_once('X'))?;
    let rows = rows.trim().parse().ok()?;
    let columns = columns.trim().parse().ok()?;
    Some((rows, columns))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_write_quit() {
        assert_eq!(parse_command("wq"), Command::WriteQuit);
        assert_eq!(parse_command("q!"), Command::Quit { force: true });
    }

    #[test]
    fn parses_table_commands() {
        assert_eq!(
            parse_command("table"),
            Command::Table {
                rows: 2,
                columns: 2
            }
        );
        assert_eq!(
            parse_command("table 3x4"),
            Command::Table {
                rows: 3,
                columns: 4
            }
        );
    }

    #[test]
    fn parses_table_mutation_commands() {
        assert_eq!(
            parse_command("row"),
            Command::TableRow {
                placement: TableRowPlacement::Below
            }
        );
        assert_eq!(
            parse_command("row above"),
            Command::TableRow {
                placement: TableRowPlacement::Above
            }
        );
        assert_eq!(
            parse_command("column"),
            Command::TableColumn {
                placement: TableColumnPlacement::Right
            }
        );
        assert_eq!(
            parse_command("col left"),
            Command::TableColumn {
                placement: TableColumnPlacement::Left
            }
        );
    }
}
