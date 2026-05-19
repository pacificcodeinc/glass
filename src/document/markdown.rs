use crate::{
    document::model::{Block, Document, Inline, ListMarker, TableAlignment, TableCell, TableRow},
    markdown::inline::{LinkKind, links},
};

pub struct MarkdownCodec;

impl MarkdownCodec {
    pub fn parse(source: &str) -> Document {
        let lines = source.lines().collect::<Vec<_>>();
        let mut blocks = Vec::new();
        let mut line = 0usize;

        while line < lines.len() {
            let current = lines[line];

            if current.trim().is_empty() {
                blocks.push(Block::Blank);
                line += 1;
                continue;
            }

            if current.trim_start().starts_with('<') && current.trim_end().ends_with('>') {
                blocks.push(Block::RawMarkdown(current.to_string()));
                line += 1;
                continue;
            }

            if let Some((language, code, next_line)) = parse_code_fence(&lines, line) {
                blocks.push(Block::CodeFence { language, code });
                line = next_line;
                continue;
            }

            if let Some((table, next_line)) = parse_table(&lines, line) {
                blocks.push(table);
                line = next_line;
                continue;
            }

            blocks.push(parse_line_block(current));
            line += 1;
        }

        if blocks.is_empty() {
            blocks.push(Block::Blank);
        }

        Document { blocks }
    }

    pub fn parse_plain(source: &str) -> Document {
        let lines = source.lines().collect::<Vec<_>>();
        let mut blocks = Vec::new();
        let mut line = 0usize;
        while line < lines.len() {
            if let Some((table, next_line)) = parse_table(&lines, line) {
                blocks.push(table);
                line = next_line;
                continue;
            }
            blocks.push(parse_facade_line_block(lines[line]));
            line += 1;
        }
        if source.ends_with('\n') {
            blocks.push(Block::Blank);
        }
        if blocks.is_empty() {
            blocks.push(Block::Blank);
        }
        Document { blocks }
    }

    pub fn serialize(document: &Document) -> String {
        let mut out = String::new();
        for (index, block) in document.blocks.iter().enumerate() {
            if index > 0 {
                out.push('\n');
            }
            out.push_str(&block.to_markdown());
        }
        out
    }
}

fn parse_line_block(line: &str) -> Block {
    let leading = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();

    if leading == 0
        && let Some((level, rest)) = parse_heading(trimmed)
    {
        return Block::Heading {
            level,
            content: parse_inlines(rest),
        };
    }

    if leading == 0 && trimmed.starts_with('>') {
        let level = trimmed.chars().take_while(|ch| *ch == '>').count() as u8;
        let rest = trimmed[level as usize..].trim_start();
        return Block::Quote {
            level,
            content: parse_inlines(rest),
        };
    }

    if let Some((checked, rest)) = parse_markdown_checkbox(trimmed) {
        return Block::ChecklistItem {
            indent: leading,
            checked,
            content: parse_inlines(rest),
        };
    }

    if let Some((number, rest)) = parse_numbered_marker(trimmed) {
        return Block::ListItem {
            indent: leading,
            marker: ListMarker::Ordered(number),
            content: parse_inlines(rest),
        };
    }

    if let Some(rest) = parse_bullet_marker(trimmed) {
        return Block::ListItem {
            indent: leading,
            marker: ListMarker::Bullet,
            content: parse_inlines(rest),
        };
    }

    Block::Paragraph(parse_inlines(line))
}

fn parse_facade_line_block(line: &str) -> Block {
    let leading = line.len() - line.trim_start().len();
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return Block::Blank;
    }

    if let Some((level, rest)) = parse_heading(trimmed) {
        return Block::Heading {
            level,
            content: parse_inlines(rest),
        };
    }

    if let Some(rest) = trimmed.strip_prefix("> ") {
        return Block::Quote {
            level: 1,
            content: parse_inlines(rest),
        };
    }

    if let Some((checked, rest)) = parse_markdown_checkbox(trimmed) {
        return Block::ChecklistItem {
            indent: leading,
            checked,
            content: parse_inlines(rest),
        };
    }

    if let Some(rest) = trimmed.strip_prefix("[ ] ") {
        return Block::ChecklistItem {
            indent: leading,
            checked: false,
            content: parse_inlines(rest),
        };
    }

    if let Some(rest) = trimmed.strip_prefix("[x] ") {
        return Block::ChecklistItem {
            indent: leading,
            checked: true,
            content: parse_inlines(rest),
        };
    }

    if let Some(rest) = trimmed
        .strip_prefix("• ")
        .or_else(|| trimmed.strip_prefix("◦ "))
        .or_else(|| parse_bullet_marker(trimmed))
    {
        return Block::ListItem {
            indent: leading,
            marker: ListMarker::Bullet,
            content: parse_inlines(rest),
        };
    }

    if let Some((number, rest)) = parse_numbered_marker(trimmed) {
        return Block::ListItem {
            indent: leading,
            marker: ListMarker::Ordered(number),
            content: parse_inlines(rest),
        };
    }

    Block::Paragraph(parse_inlines(line))
}

fn parse_heading(trimmed: &str) -> Option<(u8, &str)> {
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) || trimmed.chars().nth(level) != Some(' ') {
        return None;
    }
    Some((level as u8, trimmed[level + 1..].trim_start()))
}

fn parse_markdown_checkbox(trimmed: &str) -> Option<(bool, &str)> {
    trimmed
        .strip_prefix("- [ ] ")
        .map(|rest| (false, rest))
        .or_else(|| trimmed.strip_prefix("- [x] ").map(|rest| (true, rest)))
}

fn parse_bullet_marker(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
}

fn parse_numbered_marker(trimmed: &str) -> Option<(usize, &str)> {
    let bytes = trimmed.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || trimmed.get(i..i + 2) != Some(". ") {
        return None;
    }
    Some((trimmed[..i].parse().unwrap_or(1), &trimmed[i + 2..]))
}

fn parse_code_fence(lines: &[&str], start: usize) -> Option<(Option<String>, String, usize)> {
    let opener = lines.get(start)?.trim_start();
    let language = opener.strip_prefix("```")?.trim();
    let mut code = String::new();
    let mut line = start + 1;
    while line < lines.len() {
        if lines[line].trim_start().starts_with("```") {
            return Some((
                (!language.is_empty()).then(|| language.to_string()),
                code.trim_end_matches('\n').to_string(),
                line + 1,
            ));
        }
        code.push_str(lines[line]);
        code.push('\n');
        line += 1;
    }
    Some((
        (!language.is_empty()).then(|| language.to_string()),
        code.trim_end_matches('\n').to_string(),
        line,
    ))
}

fn parse_table(lines: &[&str], start: usize) -> Option<(Block, usize)> {
    if start + 1 >= lines.len() {
        return None;
    }
    let header = parse_table_cells(lines[start])?;
    let alignments = parse_delimiter(lines[start + 1])?;
    if header.len() < 2 || alignments.len() < 2 {
        return None;
    }

    let mut rows = vec![TableRow { cells: header }];
    let mut line = start + 2;
    while line < lines.len() {
        let Some(cells) = parse_table_cells(lines[line]) else {
            break;
        };
        if cells.len() < 2 {
            break;
        }
        rows.push(TableRow { cells });
        line += 1;
    }

    Some((Block::Table { alignments, rows }, line))
}

fn parse_table_cells(line: &str) -> Option<Vec<TableCell>> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return None;
    }

    let mut cells = trimmed
        .trim_matches('|')
        .split('|')
        .map(|cell| TableCell {
            content: parse_inlines(cell.trim()),
        })
        .collect::<Vec<_>>();
    if cells.is_empty() {
        None
    } else {
        Some(std::mem::take(&mut cells))
    }
}

fn parse_delimiter(line: &str) -> Option<Vec<TableAlignment>> {
    let cells = line.trim().trim_matches('|').split('|');
    cells
        .map(|cell| {
            let value = cell.trim();
            let left = value.starts_with(':');
            let right = value.ends_with(':');
            let dashes = value.trim_matches(':');
            if dashes.len() < 3 || !dashes.chars().all(|ch| ch == '-') {
                return None;
            }
            Some(match (left, right) {
                (true, true) => TableAlignment::Center,
                (false, true) => TableAlignment::Right,
                _ => TableAlignment::Left,
            })
        })
        .collect()
}

pub fn parse_inlines(source: &str) -> Vec<Inline> {
    let mut result = Vec::new();
    let parsed_links = links(source);
    let mut index = 0usize;

    for link in parsed_links {
        if link.source_start > index {
            result.extend(parse_styled_text(&slice_chars(
                source,
                index,
                link.source_start,
            )));
        }

        match link.kind {
            LinkKind::Markdown => result.push(Inline::Link {
                label: parse_styled_text(link.label.as_deref().unwrap_or(&link.target)),
                target: link.target,
                kind: LinkKind::Markdown,
            }),
            LinkKind::Wiki => result.push(Inline::Link {
                label: vec![Inline::Text(
                    link.label.clone().unwrap_or_else(|| link.target.clone()),
                )],
                target: link.target,
                kind: LinkKind::Wiki,
            }),
            LinkKind::Url => result.push(Inline::BareUrl(link.target)),
        }
        index = link.source_end;
    }

    if index < source.chars().count() {
        result.extend(parse_styled_text(&slice_chars(
            source,
            index,
            source.chars().count(),
        )));
    }

    merge_text(result)
}

fn parse_styled_text(source: &str) -> Vec<Inline> {
    let chars = source.chars().collect::<Vec<_>>();
    let mut result = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        if chars[index] == '`'
            && let Some(end) = find_next(&chars, index + 1, '`')
        {
            result.push(Inline::Code(chars[index + 1..end].iter().collect()));
            index = end + 1;
            continue;
        }

        if starts_with(&chars, index, "**")
            && let Some(end) = find_token(&chars, index + 2, "**")
        {
            result.push(Inline::Strong(parse_styled_text(
                &chars[index + 2..end].iter().collect::<String>(),
            )));
            index = end + 2;
            continue;
        }

        if (chars[index] == '*' || chars[index] == '_')
            && let Some(end) = find_next(&chars, index + 1, chars[index])
        {
            result.push(Inline::Emphasis(parse_styled_text(
                &chars[index + 1..end].iter().collect::<String>(),
            )));
            index = end + 1;
            continue;
        }

        let next = next_special(&chars, index + 1).unwrap_or(chars.len());
        result.push(Inline::Text(chars[index..next].iter().collect()));
        index = next;
    }

    merge_text(result)
}

fn merge_text(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut merged: Vec<Inline> = Vec::new();
    for inline in inlines {
        if let (Some(Inline::Text(left)), Inline::Text(right)) = (merged.last_mut(), &inline) {
            left.push_str(right);
        } else {
            merged.push(inline);
        }
    }
    merged
}

fn find_next(chars: &[char], start: usize, needle: char) -> Option<usize> {
    (start..chars.len()).find(|index| chars[*index] == needle)
}

fn find_token(chars: &[char], start: usize, token: &str) -> Option<usize> {
    (start..chars.len()).find(|index| starts_with(chars, *index, token))
}

fn starts_with(chars: &[char], index: usize, token: &str) -> bool {
    token
        .chars()
        .enumerate()
        .all(|(offset, ch)| chars.get(index + offset) == Some(&ch))
}

fn next_special(chars: &[char], start: usize) -> Option<usize> {
    (start..chars.len())
        .find(|index| chars[*index] == '`' || chars[*index] == '*' || chars[*index] == '_')
}

fn slice_chars(source: &str, start: usize, end: usize) -> String {
    source
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::{inline_markdown, inline_plain_text};

    #[test]
    fn parses_and_serializes_common_blocks() {
        let source = "# Title\n\n- [x] done\n\n> quote\n\n[README](README.md)";
        let document = MarkdownCodec::parse(source);

        assert_eq!(
            MarkdownCodec::serialize(&document),
            "# Title\n\n- [x] done\n\n> quote\n\n[README](README.md)"
        );
        assert_eq!(
            document.plain_text(),
            "Title\n\n[x] done\n\n> quote\n\nREADME"
        );
    }

    #[test]
    fn parses_and_serializes_tables_canonically() {
        let source = "| Name | Role |\n| --- | --- |\n| Ada | Editor |";
        let document = MarkdownCodec::parse(source);

        assert_eq!(
            MarkdownCodec::serialize(&document),
            "| Name | Role   |\n| ---- | ------ |\n| Ada  | Editor |"
        );
    }

    #[test]
    fn preserves_raw_markdown_blocks() {
        let source = "<div>raw</div>";
        let document = MarkdownCodec::parse(source);

        assert_eq!(MarkdownCodec::serialize(&document), source);
    }

    #[test]
    fn parses_facade_shortcuts() {
        let document = MarkdownCodec::parse_plain("# Heading\n[ ] todo\n• item");

        assert_eq!(
            MarkdownCodec::serialize(&document),
            "# Heading\n- [ ] todo\n- item"
        );
    }

    #[test]
    fn inline_plain_text_hides_syntax() {
        let inlines = parse_inlines("a **bold** [link](target.md)");

        assert_eq!(inline_plain_text(&inlines), "a bold link");
        assert_eq!(inline_markdown(&inlines), "a **bold** [link](target.md)");
    }
}
