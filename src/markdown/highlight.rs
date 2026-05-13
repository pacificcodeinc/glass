use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::config::theme::Theme;

pub fn render_markdown_line(source: &str, theme: Theme, active: bool) -> Line<'static> {
    if active {
        return highlight_source_line(source, theme);
    }

    let source = source.trim_end_matches(['\r', '\n']);
    let leading_width = source.len() - source.trim_start().len();
    let leading = &source[..leading_width];
    let trimmed = &source[leading_width..];

    if trimmed.starts_with("```") {
        return Line::from(Span::styled(trimmed.to_string(), theme.code_fence));
    }

    if let Some(heading_text) = trimmed.strip_prefix("# ") {
        return Line::from(vec![
            Span::raw(leading.to_string()),
            Span::styled(heading_text.to_string(), theme.heading),
        ]);
    }

    if let Some(heading_text) = trimmed.strip_prefix("## ") {
        return Line::from(vec![
            Span::raw(leading.to_string()),
            Span::styled(heading_text.to_string(), theme.heading),
        ]);
    }

    if let Some(heading_text) = trimmed.strip_prefix("### ") {
        return Line::from(vec![
            Span::raw(leading.to_string()),
            Span::styled(heading_text.to_string(), theme.heading),
        ]);
    }

    if let Some(quote_text) = trimmed.strip_prefix("> ") {
        let mut spans = vec![
            Span::raw(leading.to_string()),
            Span::styled("│ ".to_string(), theme.quote),
        ];
        spans.extend(conceal_inline(quote_text, theme, theme.quote));
        return Line::from(spans);
    }

    if let Some((marker, rest)) = checkbox_prefix(trimmed) {
        let mut spans = vec![
            Span::raw(leading.to_string()),
            Span::styled(marker.to_string(), theme.list_marker),
        ];
        spans.extend(conceal_inline(rest, theme, Style::default().fg(theme.text)));
        return Line::from(spans);
    }

    if let Some((marker, rest)) = numbered_list_prefix(trimmed) {
        let mut spans = vec![
            Span::raw(leading.to_string()),
            Span::styled(marker.to_string(), theme.list_marker),
        ];
        spans.extend(conceal_inline(rest, theme, Style::default().fg(theme.text)));
        return Line::from(spans);
    }

    if let Some(item_text) = list_item_text(trimmed) {
        let marker_len = trimmed.len() - item_text.len();
        let mut spans = vec![
            Span::raw(leading.to_string()),
            Span::styled("• ".to_string(), theme.list_marker),
        ];
        spans.extend(conceal_inline(
            &trimmed[marker_len..],
            theme,
            Style::default().fg(theme.text),
        ));
        return Line::from(spans);
    }

    Line::from(conceal_inline(
        source,
        theme,
        Style::default().fg(theme.text),
    ))
}

pub fn highlight_source_line(source: &str, theme: Theme) -> Line<'static> {
    let trimmed = source.trim_start();
    if trimmed.starts_with('#') {
        return Line::from(Span::styled(source.to_string(), theme.heading));
    }

    if trimmed.starts_with('>') {
        return Line::from(Span::styled(source.to_string(), theme.quote));
    }

    if trimmed.starts_with("```") {
        return Line::from(Span::styled(source.to_string(), theme.code_fence));
    }

    if let Some((ws_end, marker_end)) = split_list_marker(source) {
        let mut spans = vec![
            Span::raw(source[..ws_end].to_string()),
            Span::styled(source[ws_end..marker_end].to_string(), theme.list_marker),
        ];
        spans.extend(conceal_inline(
            &source[marker_end..],
            theme,
            Style::default().fg(theme.text),
        ));
        return Line::from(spans);
    }

    let mut spans = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '`' {
            let end = find_next(&chars, index + 1, '`').unwrap_or(chars.len().saturating_sub(1));
            push_slice(
                &mut spans,
                &chars,
                index,
                (end + 1).min(chars.len()),
                theme.inline_code,
            );
            index = end + 1;
            continue;
        }

        if chars[index] == '[' {
            if let Some(end) = find_link_end(&chars, index) {
                push_slice(&mut spans, &chars, index, end + 1, theme.link);
                index = end + 1;
                continue;
            }
        }

        if is_list_marker_at(&chars, index) {
            push_slice(&mut spans, &chars, index, index + 1, theme.list_marker);
            index += 1;
            continue;
        }

        if starts_with(&chars, index, "**") {
            let end = find_token(&chars, index + 2, "**").unwrap_or(index);
            let stop = if end > index { end + 2 } else { index + 2 };
            push_slice(
                &mut spans,
                &chars,
                index,
                stop.min(chars.len()),
                Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
            );
            index = stop;
            continue;
        }

        let next = next_special(&chars, index + 1).unwrap_or(chars.len());
        push_slice(
            &mut spans,
            &chars,
            index,
            next,
            Style::default().fg(theme.text),
        );
        index = next;
    }

    Line::from(spans)
}

fn push_slice(
    spans: &mut Vec<Span<'static>>,
    chars: &[char],
    start: usize,
    end: usize,
    style: Style,
) {
    let text: String = chars[start..end].iter().collect();
    spans.push(Span::styled(text, style));
}

fn find_next(chars: &[char], start: usize, needle: char) -> Option<usize> {
    (start..chars.len()).find(|index| chars[*index] == needle)
}

fn find_token(chars: &[char], start: usize, token: &str) -> Option<usize> {
    (start..chars.len()).find(|index| starts_with(chars, *index, token))
}

fn find_link_end(chars: &[char], start: usize) -> Option<usize> {
    let close_bracket = find_next(chars, start + 1, ']')?;
    if chars.get(close_bracket + 1) != Some(&'(') {
        return None;
    }

    find_next(chars, close_bracket + 2, ')')
}

fn starts_with(chars: &[char], index: usize, token: &str) -> bool {
    token
        .chars()
        .enumerate()
        .all(|(offset, ch)| chars.get(index + offset) == Some(&ch))
}

fn next_special(chars: &[char], start: usize) -> Option<usize> {
    (start..chars.len()).find(|index| {
        chars[*index] == '`'
            || chars[*index] == '['
            || starts_with(chars, *index, "**")
            || is_list_marker_at(chars, *index)
    })
}

fn is_list_marker_at(chars: &[char], index: usize) -> bool {
    let marker = chars.get(index).copied();
    let next = chars.get(index + 1).copied();
    let at_line_start = chars[..index].iter().all(|ch| ch.is_whitespace());

    at_line_start && matches!((marker, next), (Some('-' | '*' | '+'), Some(' ')))
}

fn checkbox_prefix(trimmed: &str) -> Option<(&str, &str)> {
    if let Some(rest) = trimmed
        .strip_prefix("- [ ] ")
        .or_else(|| trimmed.strip_prefix("- [x] "))
    {
        let marker_len = trimmed.len() - rest.len();
        Some((&trimmed[..marker_len], rest))
    } else {
        None
    }
}

fn numbered_list_prefix(trimmed: &str) -> Option<(&str, &str)> {
    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && trimmed.get(i..i + 2) == Some(". ") {
        Some((&trimmed[..i + 2], &trimmed[i + 2..]))
    } else {
        None
    }
}

fn list_item_text(trimmed: &str) -> Option<&str> {
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .or_else(|| trimmed.strip_prefix("+ "))
}

fn split_list_marker(source: &str) -> Option<(usize, usize)> {
    let trimmed = source.trim_start();
    let ws = source.len() - trimmed.len();

    if trimmed.starts_with("- [ ] ") || trimmed.starts_with("- [x] ") {
        return Some((ws, ws + 6));
    }
    if trimmed.starts_with("- ") || trimmed.starts_with("* ") || trimmed.starts_with("+ ") {
        return Some((ws, ws + 2));
    }

    let bytes = trimmed.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && trimmed.get(i..i + 2) == Some(". ") {
        return Some((ws, ws + i + 2));
    }

    None
}

fn conceal_inline(source: &str, theme: Theme, base_style: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = source.chars().collect();
    let mut spans = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '`' {
            if let Some(end) = find_next(&chars, index + 1, '`') {
                push_slice(&mut spans, &chars, index + 1, end, theme.inline_code);
                index = end + 1;
                continue;
            }
        }

        if starts_with(&chars, index, "**") {
            if let Some(end) = find_token(&chars, index + 2, "**") {
                push_slice(
                    &mut spans,
                    &chars,
                    index + 2,
                    end,
                    base_style.add_modifier(Modifier::BOLD),
                );
                index = end + 2;
                continue;
            }
        }

        if chars[index] == '*' || chars[index] == '_' {
            if let Some(end) = find_next(&chars, index + 1, chars[index]) {
                push_slice(
                    &mut spans,
                    &chars,
                    index + 1,
                    end,
                    base_style.add_modifier(Modifier::ITALIC),
                );
                index = end + 1;
                continue;
            }
        }

        if chars[index] == '[' {
            if let Some((close_bracket, close_paren)) = find_link_parts(&chars, index) {
                push_slice(&mut spans, &chars, index + 1, close_bracket, theme.link);
                let url_start = close_bracket + 2;
                if url_start < close_paren {
                    spans.push(Span::styled(" ".to_string(), base_style));
                    push_slice(
                        &mut spans,
                        &chars,
                        url_start,
                        close_paren,
                        Style::default().fg(theme.muted),
                    );
                }
                index = close_paren + 1;
                continue;
            }
        }

        let next = next_conceal_special(&chars, index + 1).unwrap_or(chars.len());
        push_slice(&mut spans, &chars, index, next, base_style);
        index = next;
    }

    spans
}

fn find_link_parts(chars: &[char], start: usize) -> Option<(usize, usize)> {
    let close_bracket = find_next(chars, start + 1, ']')?;
    if chars.get(close_bracket + 1) != Some(&'(') {
        return None;
    }

    let close_paren = find_next(chars, close_bracket + 2, ')')?;
    Some((close_bracket, close_paren))
}

fn next_conceal_special(chars: &[char], start: usize) -> Option<usize> {
    (start..chars.len()).find(|index| {
        chars[*index] == '`'
            || chars[*index] == '['
            || chars[*index] == '*'
            || chars[*index] == '_'
            || starts_with(chars, *index, "**")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_heading_conceals_marker() {
        let line = render_markdown_line("# Heading", Theme::monochrome_for_tests(), false);
        assert_eq!(line_text(&line), "Heading");
    }

    #[test]
    fn active_heading_keeps_marker_for_editing() {
        let line = render_markdown_line("# Heading", Theme::monochrome_for_tests(), true);
        assert_eq!(line.spans[0].content.as_ref(), "# Heading");
    }

    #[test]
    fn inactive_inline_markers_are_concealed() {
        let line = render_markdown_line("a **bold** `code`", Theme::monochrome_for_tests(), false);
        assert_eq!(line_text(&line), "a bold code");
    }

    #[test]
    fn inactive_checkbox_renders_full_marker() {
        let line = render_markdown_line("- [ ] todo", Theme::monochrome_for_tests(), false);
        assert_eq!(line_text(&line), "- [ ] todo");
    }

    #[test]
    fn inactive_checked_checkbox_renders_full_marker() {
        let line = render_markdown_line("- [x] done", Theme::monochrome_for_tests(), false);
        assert_eq!(line_text(&line), "- [x] done");
    }

    #[test]
    fn inactive_numbered_list_renders_full_marker() {
        let line = render_markdown_line("1. first item", Theme::monochrome_for_tests(), false);
        assert_eq!(line_text(&line), "1. first item");
    }

    #[test]
    fn inactive_numbered_list_multi_digit() {
        let line = render_markdown_line("10. tenth item", Theme::monochrome_for_tests(), false);
        assert_eq!(line_text(&line), "10. tenth item");
    }

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }
}
