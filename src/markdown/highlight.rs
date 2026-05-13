use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use crate::{
    config::theme::Theme,
    editor::render::{detect_list_marker, word_wrap_segments},
    markdown::inline::{InlineLink, LinkKind, links},
};

pub fn render_markdown_line(
    source: &str,
    theme: Theme,
    active: bool,
    wrap_index: usize,
) -> Line<'static> {
    render_markdown_line_with_completion(source, theme, active, wrap_index, false)
}

pub fn render_markdown_line_with_completion(
    source: &str,
    theme: Theme,
    active: bool,
    wrap_index: usize,
    completed: bool,
) -> Line<'static> {
    if active {
        return highlight_source_line(source, theme, wrap_index);
    }

    let source = source.trim_end_matches(['\r', '\n']);
    let leading_width = source.len() - source.trim_start().len();
    let leading = &source[..leading_width];
    let trimmed = &source[leading_width..];
    let allow_block_element = leading_width == 0;

    if allow_block_element && trimmed.starts_with("```") {
        return Line::from(Span::styled(trimmed.to_string(), theme.code_fence));
    }

    if allow_block_element && let Some(heading_text) = trimmed.strip_prefix("# ") {
        return Line::from(vec![
            Span::raw(leading.to_string()),
            Span::styled(heading_text.to_string(), theme.heading),
        ]);
    }

    if allow_block_element && let Some(heading_text) = trimmed.strip_prefix("## ") {
        return Line::from(vec![
            Span::raw(leading.to_string()),
            Span::styled(heading_text.to_string(), theme.heading),
        ]);
    }

    if allow_block_element && let Some(heading_text) = trimmed.strip_prefix("### ") {
        return Line::from(vec![
            Span::raw(leading.to_string()),
            Span::styled(heading_text.to_string(), theme.heading),
        ]);
    }

    if allow_block_element && let Some(quote_text) = trimmed.strip_prefix("> ") {
        let mut spans = vec![
            Span::raw(leading.to_string()),
            Span::styled("│ ".to_string(), theme.quote),
        ];
        spans.extend(conceal_inline(quote_text, theme, theme.quote));
        return Line::from(spans);
    }

    if completed {
        let mut spans = vec![Span::raw(leading.to_string())];
        spans.extend(conceal_inline(
            trimmed,
            theme,
            completed_style(Style::default().fg(theme.muted)),
        ));
        return Line::from(spans);
    }

    if wrap_index == 0 {
        if let Some((state, marker, rest)) = checkbox_prefix(trimmed) {
            let text_style = match state {
                CheckboxState::Checked => completed_style(Style::default().fg(theme.muted)),
                CheckboxState::Unchecked => Style::default().fg(theme.text),
            };
            let marker_style = match state {
                CheckboxState::Checked => theme.list_marker.add_modifier(Modifier::BOLD),
                CheckboxState::Unchecked => theme.list_marker,
            };
            let mut spans = vec![
                Span::raw(leading.to_string()),
                Span::styled(marker.to_string(), marker_style),
            ];
            spans.extend(conceal_inline(rest, theme, text_style));
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
    }

    Line::from(conceal_inline(
        source,
        theme,
        Style::default().fg(theme.text),
    ))
}

pub fn render_markdown_segment_with_completion(
    source: &str,
    segment_start: usize,
    segment_end: usize,
    theme: Theme,
    active: bool,
    wrap_index: usize,
    completed: bool,
) -> Line<'static> {
    let source_len = source.chars().count();
    let segment_start = segment_start.min(source_len);
    let segment_end = segment_end.min(source_len).max(segment_start);
    let segment = slice_chars(source, segment_start, segment_end);

    if active {
        return highlight_source_segment(source, segment_start, segment_end, theme, wrap_index);
    }

    if has_split_concealed_inline(source, segment_start, segment_end) {
        let spans = render_concealed_segment(source, segment_start, segment_end, theme, completed);
        return Line::from(spans);
    }

    render_markdown_line_with_completion(&segment, theme, false, wrap_index, completed)
}

pub fn highlight_source_line(source: &str, theme: Theme, wrap_index: usize) -> Line<'static> {
    let source_len = source.chars().count();
    highlight_source_segment(source, 0, source_len, theme, wrap_index)
}

fn highlight_source_segment(
    source: &str,
    segment_start: usize,
    segment_end: usize,
    theme: Theme,
    wrap_index: usize,
) -> Line<'static> {
    let trimmed = source.trim_start();
    let allow_block_element = source.len() == trimmed.len();
    if allow_block_element && trimmed.starts_with('#') {
        return Line::from(Span::styled(
            slice_chars(source, segment_start, segment_end),
            theme.heading,
        ));
    }

    if allow_block_element && trimmed.starts_with('>') {
        return Line::from(Span::styled(
            slice_chars(source, segment_start, segment_end),
            theme.quote,
        ));
    }

    if allow_block_element && trimmed.starts_with("```") {
        return Line::from(Span::styled(
            slice_chars(source, segment_start, segment_end),
            theme.code_fence,
        ));
    }

    if wrap_index == 0 {
        if let Some((ws_end, marker_end)) = split_list_marker(source) {
            if segment_start != 0 {
                return highlight_source_segment_without_block_rules(
                    source,
                    segment_start,
                    segment_end,
                    theme,
                );
            }
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
    }

    highlight_source_segment_without_block_rules(source, segment_start, segment_end, theme)
}

fn highlight_source_segment_without_block_rules(
    source: &str,
    segment_start: usize,
    segment_end: usize,
    theme: Theme,
) -> Line<'static> {
    let mut spans = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let parsed_links = links(source);
    let mut index = segment_start.min(chars.len());
    let segment_end = segment_end.min(chars.len()).max(index);

    while index < segment_end {
        if let Some(link) = link_containing_at(&parsed_links, index) {
            let next = push_source_link_chunk(&mut spans, &chars, &link, index, segment_end, theme);
            index = next;
            continue;
        }

        if chars[index] == '`' {
            let end = find_next(&chars, index + 1, '`').unwrap_or(chars.len().saturating_sub(1));
            push_slice(
                &mut spans,
                &chars,
                index,
                (end + 1).min(chars.len()).min(segment_end),
                theme.inline_code,
            );
            index = end + 1;
            continue;
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

        let next = next_special(&chars, &parsed_links, index + 1)
            .unwrap_or(chars.len())
            .min(segment_end);
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

fn slice_chars(source: &str, start: usize, end: usize) -> String {
    source
        .chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct VisualChar {
    ch: char,
    source_index: usize,
    style: Style,
}

fn concealed_text_with_mapping(source: &str) -> (String, Vec<usize>) {
    let chars = concealed_chars(source, Theme::system(), Style::default());
    (
        chars.iter().map(|ch| ch.ch).collect(),
        chars.iter().map(|ch| ch.source_index).collect(),
    )
}

pub fn concealed_wrap_segments(source: &str, width: usize) -> Vec<(usize, usize)> {
    let (concealed, mapping) = concealed_text_with_mapping(source);
    if concealed.is_empty() {
        return vec![(0, source.chars().count())];
    }
    let visual_segments = word_wrap_segments(&concealed, width);

    visual_segments
        .into_iter()
        .map(|(v_start, v_end)| {
            let s_start = mapping.get(v_start).copied().unwrap_or(0);
            let s_end = mapping
                .get(v_end)
                .copied()
                .unwrap_or(source.chars().count());
            (s_start, s_end)
        })
        .collect()
}

pub fn concealed_wrap_line(text: &str, width: usize) -> (Vec<(usize, usize)>, usize) {
    let marker_len = detect_list_marker(text);

    if marker_len == 0 || marker_len >= width {
        return (concealed_wrap_segments(text, width), 0);
    }

    let content = &text[marker_len..];
    if content.is_empty() {
        return (vec![(0, text.chars().count())], marker_len);
    }

    let content_width = width - marker_len;
    let content_segments = concealed_wrap_segments(content, content_width);

    let mut segments = Vec::new();
    for (i, (start, end)) in content_segments.into_iter().enumerate() {
        if i == 0 {
            segments.push((0, marker_len + end));
        } else {
            segments.push((marker_len + start, marker_len + end));
        }
    }

    (segments, marker_len)
}

fn render_concealed_segment(
    source: &str,
    segment_start: usize,
    segment_end: usize,
    theme: Theme,
    completed: bool,
) -> Vec<Span<'static>> {
    let base_style = if completed {
        completed_style(Style::default().fg(theme.muted))
    } else {
        Style::default().fg(theme.text)
    };
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;

    for visual in concealed_chars(source, theme, base_style) {
        if visual.source_index < segment_start || visual.source_index >= segment_end {
            continue;
        }

        if current_style == Some(visual.style) {
            current_text.push(visual.ch);
            continue;
        }

        if let Some(style) = current_style.take() {
            spans.push(Span::styled(std::mem::take(&mut current_text), style));
        }
        current_style = Some(visual.style);
        current_text.push(visual.ch);
    }

    if let Some(style) = current_style {
        spans.push(Span::styled(current_text, style));
    }

    spans
}

fn concealed_chars(source: &str, theme: Theme, base_style: Style) -> Vec<VisualChar> {
    let chars: Vec<char> = source.chars().collect();
    let parsed_links = links(source);
    let mut visual = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '`' {
            if let Some(end) = find_next(&chars, index + 1, '`') {
                push_visual_slice(&mut visual, &chars, index, end + 1, theme.inline_code);
                index = end + 1;
                continue;
            }
        }

        if starts_with(&chars, index, "**") {
            if let Some(end) = find_token(&chars, index + 2, "**") {
                push_visual_slice(
                    &mut visual,
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
                push_visual_slice(
                    &mut visual,
                    &chars,
                    index + 1,
                    end,
                    base_style.add_modifier(Modifier::ITALIC),
                );
                index = end + 1;
                continue;
            }
        }

        if let Some(link) = link_starting_at(&parsed_links, index) {
            push_visual_link(&mut visual, &link, &chars, theme, base_style);
            index = link.source_end;
            continue;
        }

        visual.push(VisualChar {
            ch: chars[index],
            source_index: index,
            style: base_style,
        });
        index += 1;
    }

    visual
}

fn push_visual_slice(
    visual: &mut Vec<VisualChar>,
    chars: &[char],
    start: usize,
    end: usize,
    style: Style,
) {
    for (offset, ch) in chars[start..end].iter().enumerate() {
        visual.push(VisualChar {
            ch: *ch,
            source_index: start + offset,
            style,
        });
    }
}

fn push_visual_link(
    visual: &mut Vec<VisualChar>,
    link: &InlineLink,
    chars: &[char],
    theme: Theme,
    base_style: Style,
) {
    match link.kind {
        LinkKind::Markdown | LinkKind::Wiki => {
            if let (Some(start), Some(end)) = (link.label_start, link.label_end) {
                for index in start..end {
                    if chars[index] == '`' {
                        continue;
                    }
                    visual.push(VisualChar {
                        ch: chars[index],
                        source_index: index,
                        style: link_text_style(theme, base_style),
                    });
                }
            }
        }
        LinkKind::Url => {
            let shortened = short_link_target(&link.target);
            let target_len = link.target_end.saturating_sub(link.target_start).max(1);
            let display_len = shortened.chars().count().max(1);
            for (offset, ch) in shortened.chars().enumerate() {
                let source_index = link.target_start + (offset * target_len / display_len);
                visual.push(VisualChar {
                    ch,
                    source_index: source_index.min(link.target_end.saturating_sub(1)),
                    style: link_text_style(theme, base_style),
                });
            }
        }
    }
}

fn has_split_concealed_inline(source: &str, segment_start: usize, segment_end: usize) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let parsed_links = links(source);

    for link in parsed_links {
        if ranges_overlap(
            segment_start,
            segment_end,
            link.source_start,
            link.source_end,
        ) && (link.source_start < segment_start
            || link.source_end > segment_end
            || matches!(link.kind, LinkKind::Url))
        {
            return true;
        }
    }

    let mut index = 0;
    while index < chars.len() {
        let token = if starts_with(&chars, index, "**") {
            Some("**")
        } else if chars[index] == '`' {
            Some("`")
        } else if chars[index] == '*' || chars[index] == '_' {
            Some(if chars[index] == '*' { "*" } else { "_" })
        } else {
            None
        };

        let Some(token) = token else {
            index += 1;
            continue;
        };

        let end = if token == "`" || token == "*" || token == "_" {
            find_next(&chars, index + 1, chars[index]).map(|end| end + 1)
        } else {
            find_token(&chars, index + token.chars().count(), token)
                .map(|end| end + token.chars().count())
        };

        let Some(end) = end else {
            index += token.chars().count();
            continue;
        };

        if ranges_overlap(segment_start, segment_end, index, end)
            && (index < segment_start || end > segment_end)
        {
            return true;
        }
        index = end;
    }

    false
}

fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
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

fn next_special(chars: &[char], links: &[InlineLink], start: usize) -> Option<usize> {
    (start..chars.len()).find(|index| {
        chars[*index] == '`'
            || chars[*index] == '['
            || chars[*index] == '<'
            || starts_with(chars, *index, "**")
            || is_list_marker_at(chars, *index)
            || (matches!(chars[*index], 'h' | 'w') && link_starting_at(links, *index).is_some())
    })
}

fn is_list_marker_at(chars: &[char], index: usize) -> bool {
    let marker = chars.get(index).copied();
    let next = chars.get(index + 1).copied();
    let at_line_start = chars[..index].iter().all(|ch| ch.is_whitespace());

    at_line_start && matches!((marker, next), (Some('-' | '*' | '+'), Some(' ')))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CheckboxState {
    Checked,
    Unchecked,
}

fn checkbox_prefix(trimmed: &str) -> Option<(CheckboxState, &str, &str)> {
    if let Some(rest) = trimmed.strip_prefix("- [ ] ") {
        let marker_len = trimmed.len() - rest.len();
        return Some((CheckboxState::Unchecked, &trimmed[..marker_len], rest));
    }

    let rest = trimmed.strip_prefix("- [x] ")?;
    let marker_len = trimmed.len() - rest.len();
    Some((CheckboxState::Checked, &trimmed[..marker_len], rest))
}

fn completed_style(style: Style) -> Style {
    style.add_modifier(Modifier::DIM)
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
    let parsed_links = links(source);
    let mut spans = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '`' {
            if let Some(end) = find_next(&chars, index + 1, '`') {
                push_slice(&mut spans, &chars, index, end + 1, theme.inline_code);
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

        if let Some(link) = link_starting_at(&parsed_links, index) {
            spans.extend(render_link(&link, &chars, theme, base_style));
            index = link.source_end;
            continue;
        }

        let next = next_conceal_special(&chars, &parsed_links, index + 1).unwrap_or(chars.len());
        push_slice(&mut spans, &chars, index, next, base_style);
        index = next;
    }

    spans
}

fn next_conceal_special(chars: &[char], links: &[InlineLink], start: usize) -> Option<usize> {
    (start..chars.len()).find(|index| {
        chars[*index] == '`'
            || chars[*index] == '['
            || chars[*index] == '<'
            || chars[*index] == '*'
            || chars[*index] == '_'
            || starts_with(chars, *index, "**")
            || (matches!(chars[*index], 'h' | 'w') && link_starting_at(links, *index).is_some())
    })
}

fn link_starting_at(links: &[InlineLink], index: usize) -> Option<InlineLink> {
    links
        .iter()
        .find(|link| link.source_start == index)
        .cloned()
}

fn link_containing_at(links: &[InlineLink], index: usize) -> Option<InlineLink> {
    links
        .iter()
        .find(|link| index >= link.source_start && index < link.source_end)
        .cloned()
}

fn push_source_link_chunk(
    spans: &mut Vec<Span<'static>>,
    chars: &[char],
    link: &InlineLink,
    index: usize,
    segment_end: usize,
    theme: Theme,
) -> usize {
    let (end, style, hide_backticks) = match source_link_region(link, index) {
        SourceLinkRegion::Syntax(end) => (end, Style::default().fg(theme.muted), false),
        SourceLinkRegion::Label(end) => (end, link_text_style(theme, Style::default()), true),
        SourceLinkRegion::Target(end) => (end, Style::default().fg(theme.muted), false),
    };
    let end = end.min(segment_end).min(chars.len());
    let text: String = if hide_backticks {
        chars[index..end]
            .iter()
            .map(|ch| if *ch == '`' { ' ' } else { *ch })
            .collect()
    } else {
        chars[index..end].iter().collect()
    };
    spans.push(Span::styled(text, style));
    end
}

enum SourceLinkRegion {
    Syntax(usize),
    Label(usize),
    Target(usize),
}

fn source_link_region(link: &InlineLink, index: usize) -> SourceLinkRegion {
    match link.kind {
        LinkKind::Markdown | LinkKind::Wiki => {
            let label_start = link.label_start.unwrap_or(link.source_start);
            let label_end = link.label_end.unwrap_or(label_start);
            if index < label_start {
                SourceLinkRegion::Syntax(label_start)
            } else if index < label_end {
                SourceLinkRegion::Label(label_end)
            } else if index < link.target_start {
                SourceLinkRegion::Syntax(link.target_start)
            } else if index < link.target_end {
                SourceLinkRegion::Target(link.target_end)
            } else {
                SourceLinkRegion::Syntax(link.source_end)
            }
        }
        LinkKind::Url => {
            if index < link.target_start {
                SourceLinkRegion::Syntax(link.target_start)
            } else if index < link.target_end {
                SourceLinkRegion::Label(link.target_end)
            } else {
                SourceLinkRegion::Syntax(link.source_end)
            }
        }
    }
}

fn render_link(
    link: &InlineLink,
    chars: &[char],
    theme: Theme,
    base_style: Style,
) -> Vec<Span<'static>> {
    match link.kind {
        LinkKind::Markdown => render_markdown_link(link, chars, theme, base_style),
        LinkKind::Wiki => render_wiki_link(link, chars, theme, base_style),
        LinkKind::Url => render_url_link(link, theme, base_style),
    }
}

fn render_markdown_link(
    link: &InlineLink,
    chars: &[char],
    theme: Theme,
    base_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if let (Some(label_start), Some(label_end)) = (link.label_start, link.label_end) {
        push_link_label(&mut spans, chars, label_start, label_end, theme, base_style);
    }

    spans
}

fn render_wiki_link(
    link: &InlineLink,
    chars: &[char],
    theme: Theme,
    base_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if let (Some(label_start), Some(label_end)) = (link.label_start, link.label_end) {
        push_link_label(&mut spans, chars, label_start, label_end, theme, base_style);
    }

    spans
}

fn render_url_link(link: &InlineLink, theme: Theme, base_style: Style) -> Vec<Span<'static>> {
    vec![Span::styled(
        short_link_target(&link.target),
        link_text_style(theme, base_style),
    )]
}

fn push_link_label(
    spans: &mut Vec<Span<'static>>,
    chars: &[char],
    start: usize,
    end: usize,
    theme: Theme,
    base_style: Style,
) {
    let text: String = chars[start..end].iter().filter(|ch| **ch != '`').collect();
    if !text.is_empty() {
        spans.push(Span::styled(text, link_text_style(theme, base_style)));
    }
}

fn link_text_style(theme: Theme, base_style: Style) -> Style {
    Style::default()
        .fg(theme.link.fg.unwrap_or(theme.text))
        .add_modifier(base_style.add_modifier | Modifier::UNDERLINED)
}

fn short_link_target(target: &str) -> String {
    let without_scheme = target
        .strip_prefix("https://")
        .or_else(|| target.strip_prefix("http://"))
        .unwrap_or(target);
    let without_www = without_scheme
        .strip_prefix("www.")
        .unwrap_or(without_scheme);
    let mut parts = without_www.split('/');
    let Some(host) = parts.next() else {
        return target.to_string();
    };
    let rest = parts.collect::<Vec<_>>();

    if rest.is_empty() || rest.iter().all(|part| part.is_empty()) {
        return host.to_string();
    }

    let last = rest
        .iter()
        .rev()
        .find(|part| !part.is_empty())
        .copied()
        .unwrap_or_default();
    if last.is_empty() {
        host.to_string()
    } else {
        format!("{host}/.../{last}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inactive_heading_conceals_marker() {
        let line = render_markdown_line("# Heading", Theme::monochrome_for_tests(), false, 0);
        assert_eq!(line_text(&line), "Heading");
    }

    #[test]
    fn active_heading_keeps_marker_for_editing() {
        let line = render_markdown_line("# Heading", Theme::monochrome_for_tests(), true, 0);
        assert_eq!(line.spans[0].content.as_ref(), "# Heading");
    }

    #[test]
    fn inactive_inline_code_preserves_delimiters_for_stable_width() {
        let line =
            render_markdown_line("a **bold** `code`", Theme::monochrome_for_tests(), false, 0);
        assert_eq!(line_text(&line), "a bold `code`");
    }

    #[test]
    fn inactive_markdown_link_shows_only_label() {
        let line = render_markdown_line(
            "[Glass](https://github.com/pacificcodeinc/glass)",
            Theme::monochrome_for_tests(),
            false,
            0,
        );

        assert_eq!(line_text(&line), "Glass");
        assert!(
            line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        assert_eq!(
            line.spans[0].style.fg,
            Theme::monochrome_for_tests().link.fg
        );
    }

    #[test]
    fn wrapped_markdown_link_first_segment_hides_target() {
        let source =
            "[`v0.1.2...v0.1.3`](https://github.com/pacificcodeinc/glass/compare/v0.1.2...v0.1.3)";
        let segment_end = source.chars().position(|ch| ch == '/').unwrap();

        let line = render_markdown_segment_with_completion(
            source,
            0,
            segment_end,
            Theme::monochrome_for_tests(),
            false,
            0,
            false,
        );

        assert_eq!(line_text(&line), "v0.1.2...v0.1.3");
    }

    #[test]
    fn wrapped_markdown_link_target_segment_is_hidden() {
        let source =
            "[`v0.1.2...v0.1.3`](https://github.com/pacificcodeinc/glass/compare/v0.1.2...v0.1.3)";
        let segment_start = source.chars().position(|ch| ch == '/').unwrap();

        let line = render_markdown_segment_with_completion(
            source,
            segment_start,
            source.chars().count(),
            Theme::monochrome_for_tests(),
            false,
            1,
            false,
        );

        assert_eq!(line_text(&line), "");
    }

    #[test]
    fn inactive_bare_url_is_shortened_and_underlined() {
        let line = render_markdown_line(
            "visit https://github.com/pacificcodeinc/glass/issues/123.",
            Theme::monochrome_for_tests(),
            false,
            0,
        );

        assert_eq!(line_text(&line), "visit github.com/.../123.");
        assert!(
            line.spans[1]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        assert_eq!(
            line.spans[1].style.fg,
            Theme::monochrome_for_tests().link.fg
        );
    }

    #[test]
    fn active_markdown_link_styles_label_and_revealed_target_separately() {
        let line =
            render_markdown_line("[Glass](glass.md)", Theme::monochrome_for_tests(), true, 0);

        assert_eq!(line_text(&line), "[Glass](glass.md)");
        assert!(
            line.spans[1]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        assert_eq!(
            line.spans[1].style.fg,
            Theme::monochrome_for_tests().link.fg
        );
        assert_eq!(
            line.spans[3].style.fg,
            Some(Theme::monochrome_for_tests().muted)
        );
    }

    #[test]
    fn active_markdown_link_label_backticks_are_not_visible() {
        let line = render_markdown_line(
            "[`range`](range.md)",
            Theme::monochrome_for_tests(),
            true,
            0,
        );

        assert_eq!(line_text(&line), "[ range ](range.md)");
    }

    #[test]
    fn completed_checkbox_link_remains_dimmed_and_underlined() {
        let line = render_markdown_line(
            "- [x] [Done](done.md)",
            Theme::monochrome_for_tests(),
            false,
            0,
        );

        assert_eq!(line_text(&line), "- [x] Done");
        assert!(
            line.spans[2]
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
        assert_eq!(
            line.spans[2].style.fg,
            Theme::monochrome_for_tests().link.fg
        );
        assert!(line.spans[2].style.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn inactive_indented_heading_renders_as_inline_text() {
        let line =
            render_markdown_line("  # not a heading", Theme::monochrome_for_tests(), false, 0);

        assert_eq!(line_text(&line), "  # not a heading");
        assert_eq!(
            line.spans[0].style,
            Style::default().fg(Theme::monochrome_for_tests().text)
        );
    }

    #[test]
    fn active_indented_heading_keeps_plain_source_style() {
        let line =
            render_markdown_line("  # not a heading", Theme::monochrome_for_tests(), true, 0);

        assert_eq!(line_text(&line), "  # not a heading");
        assert_eq!(
            line.spans[0].style,
            Style::default().fg(Theme::monochrome_for_tests().text)
        );
    }

    #[test]
    fn inactive_checkbox_renders_full_marker() {
        let line = render_markdown_line("- [ ] todo", Theme::monochrome_for_tests(), false, 0);
        assert_eq!(line_text(&line), "- [ ] todo");
    }

    #[test]
    fn inactive_checked_checkbox_renders_full_marker() {
        let line = render_markdown_line("- [x] done", Theme::monochrome_for_tests(), false, 0);
        assert_eq!(line_text(&line), "- [x] done");
    }

    #[test]
    fn inactive_checked_checkbox_looks_completed() {
        let line = render_markdown_line("- [x] done", Theme::monochrome_for_tests(), false, 0);

        assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
        assert!(line.spans[2].style.add_modifier.contains(Modifier::DIM));
        assert!(
            !line.spans[2]
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
    }

    #[test]
    fn inactive_unchecked_checkbox_keeps_body_normal() {
        let line = render_markdown_line("- [ ] todo", Theme::monochrome_for_tests(), false, 0);

        assert!(
            !line.spans[2]
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
    }

    #[test]
    fn checked_checkbox_continuation_line_looks_completed() {
        let line = render_markdown_line_with_completion(
            "      wrapped text",
            Theme::monochrome_for_tests(),
            false,
            1,
            true,
        );

        assert_eq!(line_text(&line), "      wrapped text");
        assert!(line.spans[1].style.add_modifier.contains(Modifier::DIM));
        assert!(
            !line.spans[1]
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
    }

    #[test]
    fn active_checked_checkbox_continuation_line_keeps_source_style() {
        let line = render_markdown_line_with_completion(
            "      wrapped text",
            Theme::monochrome_for_tests(),
            true,
            1,
            true,
        );

        assert_eq!(line_text(&line), "      wrapped text");
        assert!(
            !line.spans[0]
                .style
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
    }

    #[test]
    fn inactive_numbered_list_renders_full_marker() {
        let line = render_markdown_line("1. first item", Theme::monochrome_for_tests(), false, 0);
        assert_eq!(line_text(&line), "1. first item");
    }

    #[test]
    fn inactive_numbered_list_multi_digit() {
        let line = render_markdown_line("10. tenth item", Theme::monochrome_for_tests(), false, 0);
        assert_eq!(line_text(&line), "10. tenth item");
    }

    #[test]
    fn wrapped_markdown_link_first_segment_keeps_link_style() {
        let source =
            "[`v0.1.2...v0.1.3`](https://github.com/pacificcodeinc/glass/compare/v0.1.2...v0.1.3)";
        let segment_end = source.chars().position(|ch| ch == '/').unwrap();

        let line = render_markdown_segment_with_completion(
            source,
            0,
            segment_end,
            Theme::monochrome_for_tests(),
            false,
            0,
            false,
        );

        assert_eq!(line_text(&line), "v0.1.2...v0.1.3");
        assert!(
            line.spans.iter().any(|span| {
                span.style.add_modifier.contains(Modifier::UNDERLINED)
                    && span.style.fg == Theme::monochrome_for_tests().link.fg
            }),
            "split link label should be underlined and blue"
        );
    }

    #[test]
    fn concealed_wrap_shortens_segments_for_hidden_urls() {
        let source = "[Keep a Changelog](https://keepachangelog.com/en/1.1.0/)";
        // Raw text is 56 chars; visible text is only 16 chars.
        let raw_segments = word_wrap_segments(source, 20);
        let concealed_segments = concealed_wrap_segments(source, 20);

        assert!(
            concealed_segments.len() < raw_segments.len(),
            "concealed wrapping should produce fewer segments"
        );
        assert_eq!(concealed_segments.len(), 1);
        // The single concealed segment should cover the whole link,
        // possibly starting at the first visible character.
        assert_eq!(concealed_segments[0].1, source.chars().count());
    }

    #[test]
    fn concealed_wrap_shortens_segments_for_bare_urls() {
        let source = "visit https://github.com/pacificcodeinc/glass/issues/123.";
        let raw_segments = word_wrap_segments(source, 20);
        let concealed_segments = concealed_wrap_segments(source, 20);

        assert!(
            concealed_segments.len() < raw_segments.len(),
            "bare URL wrapping should use the shortened visible target"
        );
    }

    #[test]
    fn wrapped_bold_text_keeps_delimiters_hidden_across_segments() {
        let source = "**alpha beta gamma**";
        let segments = concealed_wrap_segments(source, 8);

        assert!(segments.len() > 1);
        for (index, (start, end)) in segments.into_iter().enumerate() {
            let line = render_markdown_segment_with_completion(
                source,
                start,
                end,
                Theme::monochrome_for_tests(),
                false,
                index,
                false,
            );
            let text = line_text(&line);

            assert!(!text.contains('*'));
            assert!(
                line.spans
                    .iter()
                    .any(|span| span.style.add_modifier.contains(Modifier::BOLD))
            );
        }
    }

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }
}
