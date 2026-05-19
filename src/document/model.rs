use crate::markdown::inline::LinkKind;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Blank,
    Paragraph(Vec<Inline>),
    Heading {
        level: u8,
        content: Vec<Inline>,
    },
    Quote {
        level: u8,
        content: Vec<Inline>,
    },
    ListItem {
        indent: usize,
        marker: ListMarker,
        content: Vec<Inline>,
    },
    ChecklistItem {
        indent: usize,
        checked: bool,
        content: Vec<Inline>,
    },
    CodeFence {
        language: Option<String>,
        code: String,
    },
    Table {
        alignments: Vec<TableAlignment>,
        rows: Vec<TableRow>,
    },
    RawMarkdown(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMarker {
    Bullet,
    Ordered(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Code(String),
    Link {
        label: Vec<Inline>,
        target: String,
        kind: LinkKind,
    },
    BareUrl(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
}

impl Default for TableAlignment {
    fn default() -> Self {
        Self::Left
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableCell {
    pub content: Vec<Inline>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocRange {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocLink {
    pub target: String,
    pub kind: LinkKind,
}

impl Document {
    pub fn plain_text(&self) -> String {
        self.blocks
            .iter()
            .flat_map(Block::plain_lines)
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn block_for_plain_line(&self, line: usize) -> Option<&Block> {
        let mut cursor = 0usize;
        for block in &self.blocks {
            let count = block.plain_line_count();
            if line < cursor + count {
                return Some(block);
            }
            cursor += count;
        }
        None
    }

    pub fn link_at_plain_position(&self, line: usize, column: usize) -> Option<DocLink> {
        let block = self.block_for_plain_line(line)?;
        block.link_at_column(column)
    }

    pub fn serialize_range(&self, range: DocRange) -> String {
        let range_start = range.start.min(range.end);
        let range_end = range.end.max(range.start);
        if range_start == range_end {
            return String::new();
        }

        let mut result = String::new();
        let mut plain_cursor = 0usize;
        for block in &self.blocks {
            let plain = block.plain_lines().join("\n");
            let block_start = plain_cursor;
            let block_end = block_start + plain.chars().count();
            if ranges_overlap(range_start, range_end, block_start, block_end) {
                if range_start <= block_start && range_end >= block_end {
                    if !result.is_empty() {
                        result.push_str("\n\n");
                    }
                    result.push_str(&block.to_markdown());
                } else {
                    let local_start = range_start.saturating_sub(block_start);
                    let local_end = range_end.min(block_end).saturating_sub(block_start);
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&slice_chars(&plain, local_start, local_end));
                }
            }
            plain_cursor = block_end + 1;
        }

        result
    }
}

impl Block {
    pub fn plain_line_count(&self) -> usize {
        self.plain_lines().len().max(1)
    }

    pub fn plain_lines(&self) -> Vec<String> {
        match self {
            Block::Blank => vec![String::new()],
            Block::Paragraph(content) => vec![inline_plain_text(content)],
            Block::Heading { content, .. } => vec![inline_plain_text(content)],
            Block::Quote { level, content } => {
                vec![format!(
                    "{} {}",
                    quote_marker(*level),
                    inline_plain_text(content)
                )]
            }
            Block::ListItem {
                indent,
                marker,
                content,
            } => vec![format!(
                "{}{} {}",
                " ".repeat(*indent),
                marker.plain_marker(*indent),
                inline_plain_text(content)
            )],
            Block::ChecklistItem {
                indent,
                checked,
                content,
            } => vec![format!(
                "{}{} {}",
                " ".repeat(*indent),
                if *checked { "[x]" } else { "[ ]" },
                inline_plain_text(content)
            )],
            Block::CodeFence { code, .. } => {
                if code.is_empty() {
                    vec![String::new()]
                } else {
                    code.lines().map(ToOwned::to_owned).collect()
                }
            }
            Block::Table { rows, .. } => rows
                .is_empty()
                .then(Vec::new)
                .unwrap_or_else(|| self.to_markdown().lines().map(ToOwned::to_owned).collect()),
            Block::RawMarkdown(markdown) => markdown.lines().map(ToOwned::to_owned).collect(),
        }
    }

    pub fn to_markdown(&self) -> String {
        match self {
            Block::Blank => String::new(),
            Block::Paragraph(content) => inline_markdown(content),
            Block::Heading { level, content } => {
                format!(
                    "{} {}",
                    "#".repeat(*level as usize),
                    inline_markdown(content)
                )
            }
            Block::Quote { level, content } => {
                format!(
                    "{} {}",
                    ">".repeat(*level as usize),
                    inline_markdown(content)
                )
            }
            Block::ListItem {
                indent,
                marker,
                content,
            } => format!(
                "{}{} {}",
                " ".repeat(*indent),
                marker.markdown_marker(),
                inline_markdown(content)
            ),
            Block::ChecklistItem {
                indent,
                checked,
                content,
            } => format!(
                "{}- [{}] {}",
                " ".repeat(*indent),
                if *checked { "x" } else { " " },
                inline_markdown(content)
            ),
            Block::CodeFence { language, code } => {
                format!(
                    "```{}\n{}\n```",
                    language.as_deref().unwrap_or_default(),
                    code.trim_end_matches('\n')
                )
            }
            Block::Table { alignments, rows } => table_markdown(alignments, rows),
            Block::RawMarkdown(markdown) => markdown.clone(),
        }
    }

    fn link_at_column(&self, column: usize) -> Option<DocLink> {
        match self {
            Block::Paragraph(content)
            | Block::Heading { content, .. }
            | Block::Quote { content, .. }
            | Block::ListItem { content, .. }
            | Block::ChecklistItem { content, .. } => inline_link_at_column(content, column),
            Block::Table { rows, .. } => {
                let mut cursor = 0usize;
                for row in rows {
                    for cell in &row.cells {
                        let text = inline_plain_text(&cell.content);
                        if column >= cursor && column < cursor + text.chars().count() {
                            return inline_link_at_column(&cell.content, column - cursor);
                        }
                        cursor += text.chars().count() + 2;
                    }
                }
                None
            }
            _ => None,
        }
    }
}

impl ListMarker {
    fn markdown_marker(self) -> String {
        match self {
            ListMarker::Bullet => "-".to_string(),
            ListMarker::Ordered(number) => format!("{number}."),
        }
    }

    fn plain_marker(self, indent: usize) -> String {
        match self {
            ListMarker::Bullet => {
                if indent >= 2 {
                    "◦".to_string()
                } else {
                    "•".to_string()
                }
            }
            ListMarker::Ordered(number) => format!("{number}."),
        }
    }
}

pub fn inline_plain_text(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(value) | Inline::Code(value) | Inline::BareUrl(value) => {
                text.push_str(value)
            }
            Inline::Emphasis(children) | Inline::Strong(children) => {
                text.push_str(&inline_plain_text(children))
            }
            Inline::Link {
                label,
                target,
                kind,
            } => {
                if label.is_empty() && !matches!(kind, LinkKind::Wiki) {
                    text.push_str(target);
                } else {
                    text.push_str(&inline_plain_text(label));
                }
            }
        }
    }
    text
}

pub fn inline_markdown(inlines: &[Inline]) -> String {
    let mut text = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(value) => text.push_str(value),
            Inline::Emphasis(children) => {
                text.push('*');
                text.push_str(&inline_markdown(children));
                text.push('*');
            }
            Inline::Strong(children) => {
                text.push_str("**");
                text.push_str(&inline_markdown(children));
                text.push_str("**");
            }
            Inline::Code(value) => {
                text.push('`');
                text.push_str(value);
                text.push('`');
            }
            Inline::Link {
                label,
                target,
                kind,
            } => match kind {
                LinkKind::Markdown => {
                    text.push('[');
                    text.push_str(&inline_markdown(label));
                    text.push_str("](");
                    text.push_str(target);
                    text.push(')');
                }
                LinkKind::Wiki => {
                    text.push_str("[[");
                    text.push_str(target);
                    text.push_str("]]");
                }
                LinkKind::Url => text.push_str(target),
            },
            Inline::BareUrl(value) => text.push_str(value),
        }
    }
    text
}

fn inline_link_at_column(inlines: &[Inline], column: usize) -> Option<DocLink> {
    let mut cursor = 0usize;
    for inline in inlines {
        let len = inline_plain_text(std::slice::from_ref(inline))
            .chars()
            .count();
        match inline {
            Inline::Link { target, kind, .. } => {
                if column >= cursor && column < cursor + len {
                    return Some(DocLink {
                        target: target.clone(),
                        kind: kind.clone(),
                    });
                }
            }
            Inline::BareUrl(target) => {
                if column >= cursor && column < cursor + len {
                    return Some(DocLink {
                        target: target.clone(),
                        kind: LinkKind::Url,
                    });
                }
            }
            Inline::Emphasis(children) | Inline::Strong(children) => {
                if column >= cursor
                    && column < cursor + len
                    && let Some(link) = inline_link_at_column(children, column - cursor)
                {
                    return Some(link);
                }
            }
            _ => {}
        }
        cursor += len;
    }
    None
}

fn table_markdown(alignments: &[TableAlignment], rows: &[TableRow]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let column_count = rows.iter().map(|row| row.cells.len()).max().unwrap_or(0);
    if column_count == 0 {
        return String::new();
    }

    let mut widths = vec![3usize; column_count];
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate() {
            widths[index] = widths[index].max(inline_markdown(&cell.content).chars().count());
        }
    }

    let mut lines = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        lines.push(table_row_markdown(row, &widths));
        if row_index == 0 {
            lines.push(table_delimiter_markdown(alignments, &widths));
        }
    }
    lines.join("\n")
}

fn table_row_markdown(row: &TableRow, widths: &[usize]) -> String {
    let mut line = String::from("|");
    for (index, width) in widths.iter().enumerate() {
        let value = row
            .cells
            .get(index)
            .map(|cell| inline_markdown(&cell.content))
            .unwrap_or_default();
        line.push(' ');
        line.push_str(&value);
        line.push_str(&" ".repeat(width.saturating_sub(value.chars().count())));
        line.push_str(" |");
    }
    line
}

fn table_delimiter_markdown(alignments: &[TableAlignment], widths: &[usize]) -> String {
    let mut line = String::from("|");
    for (index, width) in widths.iter().enumerate() {
        let marker = match alignments.get(index).copied().unwrap_or_default() {
            TableAlignment::Left => format!(" {}", "-".repeat(*width)),
            TableAlignment::Center => format!(":{}:", "-".repeat((*width).max(3))),
            TableAlignment::Right => format!("{}:", "-".repeat(width.saturating_sub(1).max(3))),
        };
        line.push_str(&marker);
        line.push_str(" |");
    }
    line
}

fn quote_marker(level: u8) -> String {
    ">".repeat(level.max(1) as usize)
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
}

fn slice_chars(source: &str, start: usize, end: usize) -> String {
    source
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}
