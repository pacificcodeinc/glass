use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::Paragraph,
};

use crate::{
    app::{App, Mode, SearchMatch, TextSelection},
    config::theme::Theme,
    document::{Block, SurfaceMode, surface::wrap_surface_or_facade_line},
    editor::{buffer::DocumentBuffer, render::visible_rows},
    markdown::{highlight::render_markdown_segment_with_completion, table::TableLayout},
};

#[cfg(test)]
use crate::markdown::highlight::{concealed_wrap_line, concealed_wrap_segments};

const ARTICLE_WIDTH: u16 = 82;

pub fn page_area(area: Rect) -> Rect {
    let width = ARTICLE_WIDTH.min(area.width.saturating_sub(4)).max(20);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y,
        width,
        height: area.height,
    }
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App, theme: Theme) {
    let page = page_area(area);
    let line_count = app.buffer.line_count();
    let gutter_width: u16 = (line_count.to_string().len() + 1) as u16;
    let text_width = page.width.saturating_sub(gutter_width).max(1) as usize;
    let table_layout = TableLayout::new(&app.buffer);

    frame.render_widget(
        ratatui::widgets::Clear,
        Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: area.height,
        },
    );
    let rows = visible_rows(
        &app.buffer,
        app.viewport.top_line,
        app.viewport.top_wrap_index,
        page.height as usize,
        text_width,
        |line_num, text, w| {
            surface_wrap_line(
                &app.buffer,
                &table_layout,
                Some((app.cursor.line, app.cursor.column)),
                line_num,
                text,
                w,
            )
        },
    );
    let visual_range = app.visual_line_anchor.map(|anchor| {
        let start = anchor.min(app.cursor.line);
        let end = anchor.max(app.cursor.line);
        start..=end
    });

    let cursor_line_text = app.buffer.line(app.cursor.line);
    let (cursor_segments, _) = surface_wrap_line(
        &app.buffer,
        &table_layout,
        Some((app.cursor.line, app.cursor.column)),
        app.cursor.line,
        &cursor_line_text,
        text_width,
    );
    let wrap_index_of_cursor = wrap_index_for_segments(&cursor_segments, app.cursor.column);
    let mut cursor_visual_y: usize = 0;
    let mut cursor_visual_x: Option<u16> = None;
    let mut cursor_found = false;

    let height = page.height as usize;
    let mut lines = Vec::new();
    for row in &rows {
        let is_cursor_row =
            row.line_number == app.cursor.line && row.wrap_index == wrap_index_of_cursor;
        let active = row.line_number == app.cursor.line;

        let table_row = table_layout.render_row_segment(
            row.line_number,
            &row.full_text,
            text_width,
            theme,
            row.wrap_index,
        );

        let mut display_range = None;
        let (mut line, source_map) = if let Some(rendered) = table_row {
            (rendered.line, Some(rendered.source_map))
        } else if active && let Some(block) = app.buffer.block_for_line(row.line_number) {
            let surface = app.buffer.surface_line(
                row.line_number,
                SurfaceMode::Active {
                    cursor_column: app.cursor.column,
                },
            );
            if surface.has_revealed_syntax() {
                let display_segments = surface.wrap_display_segments(text_width);
                let (display_start, display_end) = display_segments
                    .get(row.wrap_index)
                    .copied()
                    .unwrap_or((0, surface.display_len()));
                display_range = Some((display_start, display_end));
                (
                    render_surface_segment(
                        block,
                        &surface.text,
                        display_start,
                        display_end,
                        theme,
                        row.wrap_index,
                        row.completed && row.wrap_index > 0,
                    ),
                    Some(surface.source_map_for_display_range(display_start, display_end)),
                )
            } else {
                (
                    render_document_segment(
                        block,
                        &row.full_text,
                        row.source_start,
                        row.source_end,
                        theme,
                    ),
                    None,
                )
            }
        } else if let Some(block) = app.buffer.block_for_line(row.line_number) {
            (
                render_document_segment(
                    block,
                    &row.full_text,
                    row.source_start,
                    row.source_end,
                    theme,
                ),
                None,
            )
        } else {
            (
                render_markdown_segment_with_completion(
                    &row.full_text,
                    row.source_start,
                    row.source_end,
                    theme,
                    active,
                    row.wrap_index,
                    row.completed && row.wrap_index > 0,
                ),
                None,
            )
        };

        if !app.search.query.is_empty() {
            let mut ranges = search_ranges_for_row(
                &app.search.matches,
                row.line_number,
                row.source_start,
                row.source_end,
            );
            if let Some(source_map) = &source_map {
                ranges = source_ranges_to_visual_ranges(source_map, row.source_start, &ranges);
            }
            line = highlight_search_ranges(line, &ranges, theme);
        }

        if let Some(selection) = app.text_selection {
            let mut ranges = selection_ranges_for_row(
                selection,
                row.line_number,
                row.source_start,
                row.source_end,
            );
            if let Some(source_map) = &source_map {
                ranges = source_ranges_to_visual_ranges(source_map, row.source_start, &ranges);
            }
            line = highlight_search_ranges(line, &ranges, theme);
        }

        if row.continuation_indent > 0 {
            let indent = Span::raw(" ".repeat(row.continuation_indent));
            line.spans.insert(0, indent);
        }

        if visual_range
            .as_ref()
            .is_some_and(|range| range.contains(&row.line_number))
        {
            line = selected_line(line, theme);
        }
        if is_cursor_row && app.mode != Mode::Visual {
            line.style = line.style.bg(theme.background);
        }

        line = add_gutter(
            line,
            gutter_width,
            Some((row.line_number, row.wrap_index)),
            app,
            theme,
        );

        if is_cursor_row && !cursor_found && lines.len() < height {
            cursor_visual_y = lines.len();
            if let Some((display_start, display_end)) = display_range {
                let surface_column = app
                    .surface_cursor_column_for_line(row.line_number)
                    .unwrap_or_else(|| {
                        app.buffer
                            .surface_line(
                                row.line_number,
                                SurfaceMode::Active {
                                    cursor_column: app.cursor.column,
                                },
                            )
                            .display_column_for_source_column(app.cursor.column)
                    });
                cursor_visual_x = Some(
                    gutter_width
                        + surface_column
                            .saturating_sub(display_start)
                            .min(display_end.saturating_sub(display_start))
                            as u16,
                );
            } else if let Some(source_map) = &source_map {
                let source_column = app.cursor.column;
                let mapped_column = source_map
                    .iter()
                    .position(|source_index| {
                        source_index.is_some_and(|index| index >= source_column)
                    })
                    .unwrap_or(source_map.len());
                cursor_visual_x = Some(gutter_width + mapped_column as u16);
            }
            cursor_found = true;
        }

        push_line(&mut lines, line, height);
    }

    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().bg(theme.background).fg(theme.text));
    frame.render_widget(paragraph, page);

    if app.mode != Mode::CommandLine {
        let cursor_indent = if wrap_index_of_cursor > 0 {
            facade_list_marker_len(&cursor_line_text)
        } else {
            0
        };
        let x = cursor_visual_x.unwrap_or_else(|| {
            column_in_segments(&cursor_segments, app.cursor.column) as u16
                + gutter_width
                + cursor_indent as u16
        });
        let y = cursor_visual_y as u16;
        frame.set_cursor_position(Position::new(page.x + x, page.y + y));
    }
}

pub(crate) fn surface_wrap_line(
    buffer: &DocumentBuffer,
    table_layout: &TableLayout,
    active_line: Option<(usize, usize)>,
    line_number: usize,
    text: &str,
    width: usize,
) -> (Vec<(usize, usize)>, usize) {
    let trimmed = text.trim_end_matches(['\r', '\n']);
    if table_layout.is_table_row(line_number) {
        return table_layout.wrap_line(line_number, trimmed, width);
    }

    let mode = active_line
        .filter(|(line, _)| *line == line_number)
        .map(|(_, cursor_column)| SurfaceMode::Active { cursor_column })
        .unwrap_or(SurfaceMode::Inactive);
    wrap_surface_or_facade_line(buffer.block_for_line(line_number), trimmed, width, mode)
}

fn render_surface_segment(
    block: &Block,
    source: &str,
    segment_start: usize,
    segment_end: usize,
    theme: Theme,
    wrap_index: usize,
    completed: bool,
) -> Line<'static> {
    match block {
        Block::Heading { .. } => {
            let mut line = render_markdown_segment_with_completion(
                source,
                segment_start,
                segment_end,
                theme,
                true,
                wrap_index,
                completed,
            );
            for span in &mut line.spans {
                span.style = span.style.patch(theme.heading);
            }
            line
        }
        Block::CodeFence { .. } => Line::from(Span::styled(
            char_slice(source, segment_start, segment_end),
            theme.inline_code,
        )),
        _ => render_markdown_segment_with_completion(
            source,
            segment_start,
            segment_end,
            theme,
            true,
            wrap_index,
            completed,
        ),
    }
}

#[cfg(test)]
fn wrap_facade_text_line(text: &str, width: usize) -> (Vec<(usize, usize)>, usize) {
    let marker_len = facade_list_marker_len(text);
    let width = width.max(1);
    if marker_len == 0 || marker_len >= width {
        return concealed_wrap_line(text, width);
    }

    let source_len = text.chars().count();
    let content = char_slice(text, marker_len, source_len);
    if content.is_empty() {
        return (vec![(0, source_len)], marker_len);
    }

    let content_width = width - marker_len;
    let content_segments = concealed_wrap_segments(&content, content_width);
    let mut segments = Vec::new();
    for (index, (start, end)) in content_segments.into_iter().enumerate() {
        if index == 0 {
            segments.push((0, marker_len + end));
        } else {
            segments.push((marker_len + start, marker_len + end));
        }
    }

    (segments, marker_len)
}

fn wrap_index_for_segments(segments: &[(usize, usize)], column: usize) -> usize {
    for (index, &(start, end)) in segments.iter().enumerate() {
        if column >= start && column < end {
            return index;
        }
    }
    segments.len().saturating_sub(1)
}

fn column_in_segments(segments: &[(usize, usize)], column: usize) -> usize {
    for &(start, end) in segments {
        if column >= start && column < end {
            return column.saturating_sub(start);
        }
    }

    if let Some(&(start, end)) = segments.last() {
        return (end - start).min(column.saturating_sub(start));
    }

    0
}

fn render_document_segment(
    block: &Block,
    source: &str,
    segment_start: usize,
    segment_end: usize,
    theme: Theme,
) -> Line<'static> {
    let text = char_slice(source, segment_start, segment_end);
    match block {
        Block::Heading { .. } => Line::from(Span::styled(text, theme.heading)),
        Block::Quote { .. } => render_markdown_segment_with_completion(
            source,
            segment_start,
            segment_end,
            theme,
            false,
            0,
            false,
        ),
        Block::CodeFence { .. } => Line::from(Span::styled(text, theme.inline_code)),
        Block::RawMarkdown(_) => Line::from(Span::styled(text, theme.muted)),
        Block::ListItem { .. } | Block::ChecklistItem { .. } => {
            let marker_len = facade_list_marker_len(source);
            let completed = matches!(block, Block::ChecklistItem { checked: true, .. });
            if marker_len <= segment_start {
                return render_markdown_segment_with_completion(
                    source,
                    segment_start,
                    segment_end,
                    theme,
                    false,
                    0,
                    completed,
                );
            }
            if marker_len >= segment_end {
                let marker_style = if completed {
                    theme.list_marker.add_modifier(Modifier::BOLD)
                } else {
                    theme.list_marker
                };
                return Line::from(Span::styled(text, marker_style));
            }

            let marker_style = if completed {
                theme.list_marker.add_modifier(Modifier::BOLD)
            } else {
                theme.list_marker
            };
            let mut spans = vec![Span::styled(
                char_slice(source, segment_start, marker_len),
                marker_style,
            )];
            spans.extend(
                render_markdown_segment_with_completion(
                    source,
                    marker_len,
                    segment_end,
                    theme,
                    false,
                    0,
                    completed,
                )
                .spans,
            );
            Line::from(spans)
        }
        _ => render_markdown_segment_with_completion(
            source,
            segment_start,
            segment_end,
            theme,
            false,
            0,
            false,
        ),
    }
}

fn facade_list_marker_len(source: &str) -> usize {
    let leading = source.chars().take_while(|ch| ch.is_whitespace()).count();
    let trimmed = char_slice(source, leading, source.chars().count());
    if trimmed.starts_with("[ ] ") || trimmed.starts_with("[x] ") {
        return leading + 4;
    }
    if trimmed.starts_with("• ") || trimmed.starts_with("◦ ") {
        return leading + 2;
    }

    let digits = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0 && char_slice(&trimmed, digits, digits + 2) == ". " {
        return leading + digits + 2;
    }

    0
}

fn selected_line(mut line: Line<'static>, theme: Theme) -> Line<'static> {
    line.style = theme.selection;
    for span in &mut line.spans {
        span.style = theme.selection;
    }
    line
}

fn push_line(lines: &mut Vec<Line<'static>>, line: Line<'static>, height: usize) -> bool {
    if lines.len() >= height {
        return false;
    }

    lines.push(line);
    true
}

fn add_gutter(
    line: Line<'static>,
    gutter_width: u16,
    source_position: Option<(usize, usize)>,
    app: &App,
    theme: Theme,
) -> Line<'static> {
    if gutter_width == 0 {
        return line;
    }

    let show_visual_line_number =
        source_position.is_some_and(|(_, wrap_index)| wrap_index == 0 && app.mode == Mode::Visual);
    let gutter = if show_visual_line_number {
        let (line_number, _) = source_position.expect("source position checked above");
        format!("{:>w$} ", line_number + 1, w = gutter_width as usize - 1)
    } else {
        " ".repeat(gutter_width as usize)
    };
    let mut spans = vec![Span::styled(gutter, Style::default().fg(theme.muted))];
    spans.extend(line.spans);
    Line::from(spans)
}

fn highlight_search_ranges(
    mut line: Line<'static>,
    ranges: &[(usize, usize)],
    theme: Theme,
) -> Line<'static> {
    if ranges.is_empty() {
        return line;
    }

    let ranges = merged_ranges(ranges);
    if ranges.is_empty() {
        return line;
    }

    let original_spans = std::mem::take(&mut line.spans);
    let mut highlighted = Vec::new();
    let mut span_start = 0usize;

    for span in original_spans {
        let text = span.content.into_owned();
        let span_len = text.chars().count();
        let span_end = span_start + span_len;
        let mut local_cursor = 0usize;

        for &(range_start, range_end) in &ranges {
            if range_end <= span_start || range_start >= span_end {
                continue;
            }

            let local_start = range_start.max(span_start) - span_start;
            let local_end = range_end.min(span_end) - span_start;
            if local_cursor < local_start {
                highlighted.push(Span::styled(
                    char_slice(&text, local_cursor, local_start),
                    span.style,
                ));
            }
            highlighted.push(Span::styled(
                char_slice(&text, local_start, local_end),
                theme.selection,
            ));
            local_cursor = local_end;
        }

        if local_cursor < span_len {
            highlighted.push(Span::styled(
                char_slice(&text, local_cursor, span_len),
                span.style,
            ));
        }

        span_start = span_end;
    }

    line.spans = highlighted;
    line
}

fn search_ranges_for_row(
    matches: &[SearchMatch],
    line_number: usize,
    source_start: usize,
    source_end: usize,
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    for search_match in matches {
        if search_match.end_line < line_number || search_match.line > line_number {
            continue;
        }

        let start = if search_match.line == line_number {
            search_match.column
        } else {
            source_start
        };
        let end = if search_match.end_line == line_number {
            search_match.end_column
        } else {
            source_end
        };

        let start = start.max(source_start);
        let end = end.min(source_end);
        if start < end {
            ranges.push((start - source_start, end - source_start));
        }
    }

    ranges
}

fn selection_ranges_for_row(
    selection: TextSelection,
    line_number: usize,
    source_start: usize,
    source_end: usize,
) -> Vec<(usize, usize)> {
    let (start_cursor, end_cursor) = selection.ordered();
    if end_cursor.line < line_number || start_cursor.line > line_number {
        return Vec::new();
    }

    let start = if start_cursor.line == line_number {
        start_cursor.column
    } else {
        source_start
    };
    let end = if end_cursor.line == line_number {
        end_cursor.column
    } else {
        source_end
    };

    let start = start.max(source_start);
    let end = end.min(source_end);
    if start < end {
        vec![(start - source_start, end - source_start)]
    } else {
        Vec::new()
    }
}

fn source_ranges_to_visual_ranges(
    source_map: &[Option<usize>],
    source_start: usize,
    source_ranges: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut active_start = None;

    for (visual_index, source_index) in source_map.iter().copied().enumerate() {
        let selected = source_index
            .and_then(|source_index| source_index.checked_sub(source_start))
            .is_some_and(|local_source| {
                source_ranges
                    .iter()
                    .any(|(start, end)| local_source >= *start && local_source < *end)
            });

        match (active_start, selected) {
            (None, true) => active_start = Some(visual_index),
            (Some(start), false) => {
                ranges.push((start, visual_index));
                active_start = None;
            }
            _ => {}
        }
    }

    if let Some(start) = active_start {
        ranges.push((start, source_map.len()));
    }

    ranges
}

fn merged_ranges(ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut ranges = ranges
        .iter()
        .copied()
        .filter(|(start, end)| start < end)
        .collect::<Vec<_>>();
    ranges.sort_unstable_by_key(|(start, end)| (*start, *end));

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some((_, last_end)) = merged.last_mut() {
            if start <= *last_end {
                *last_end = (*last_end).max(end);
                continue;
            }
        }

        merged.push((start, end));
    }

    merged
}

fn char_slice(text: &str, start: usize, end: usize) -> String {
    text.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_highlight_splits_requested_ranges() {
        let theme = Theme::monochrome_for_tests();
        let line = Line::from(vec![
            Span::styled("before ", Style::default().fg(theme.text)),
            Span::styled("needle after", Style::default().fg(theme.text)),
        ]);

        let line = highlight_search_ranges(line, &[(7, 13)], theme);

        assert_eq!(line.spans.len(), 3);
        assert_eq!(line.spans[1].content.as_ref(), "needle");
        assert_eq!(line.spans[1].style, theme.selection);
    }

    #[test]
    fn search_ranges_split_across_wrapped_rows() {
        let search_match = SearchMatch {
            line: 0,
            column: 5,
            end_line: 0,
            end_column: 12,
        };

        assert_eq!(
            search_ranges_for_row(&[search_match], 0, 0, 8),
            vec![(5, 8)]
        );
        assert_eq!(
            search_ranges_for_row(&[search_match], 0, 8, 16),
            vec![(0, 4)]
        );
    }

    #[test]
    fn selection_ranges_split_across_wrapped_rows() {
        let selection = TextSelection {
            anchor: crate::editor::cursor::Cursor { line: 0, column: 2 },
            head: crate::editor::cursor::Cursor { line: 0, column: 9 },
        };

        assert_eq!(selection_ranges_for_row(selection, 0, 0, 6), vec![(2, 6)]);
        assert_eq!(selection_ranges_for_row(selection, 0, 6, 12), vec![(0, 3)]);
    }

    #[test]
    fn source_ranges_map_to_table_visual_ranges() {
        let source_map = vec![
            None,
            Some(2),
            Some(3),
            None,
            None,
            Some(8),
            Some(9),
            Some(10),
        ];

        assert_eq!(
            source_ranges_to_visual_ranges(&source_map, 0, &[(2, 4), (9, 11)]),
            vec![(1, 3), (6, 8)]
        );
    }

    #[test]
    fn unicode_list_marker_does_not_style_body_prefix() {
        let theme = Theme::monochrome_for_tests();
        let block = Block::ListItem {
            indent: 0,
            marker: crate::document::model::ListMarker::Bullet,
            content: Vec::new(),
        };

        let line = render_document_segment(&block, "• inactive", 0, 10, theme);

        assert_eq!(line.spans[0].content.as_ref(), "• ");
        assert_eq!(line.spans[0].style, theme.list_marker);
        assert_eq!(line.spans[1].content.as_ref(), "inactive");
        assert_eq!(line.spans[1].style, Style::default().fg(theme.text));
    }

    #[test]
    fn checklist_marker_is_styled_as_one_unit() {
        let theme = Theme::monochrome_for_tests();
        let block = Block::ChecklistItem {
            indent: 0,
            checked: false,
            content: Vec::new(),
        };

        let line = render_document_segment(&block, "[ ] task", 0, 8, theme);

        assert_eq!(line.spans[0].content.as_ref(), "[ ] ");
        assert_eq!(line.spans[0].style, theme.list_marker);
        assert_eq!(line.spans[1].content.as_ref(), "task");
    }

    #[test]
    fn active_heading_surface_reveals_marker() {
        let theme = Theme::monochrome_for_tests();
        let document = crate::document::MarkdownCodec::parse("# Heading");
        let block = document.block_for_plain_line(0).unwrap();
        let surface = crate::document::SurfaceLine::for_block(
            block,
            "Heading",
            SurfaceMode::Active { cursor_column: 0 },
        );

        let line = render_surface_segment(
            block,
            &surface.text,
            0,
            surface.display_len(),
            theme,
            0,
            false,
        );

        assert_eq!(line_text(&line), "# Heading");
    }

    #[test]
    fn paragraph_render_matches_concealed_wrap_boundaries() {
        let theme = Theme::monochrome_for_tests();
        let block = Block::Paragraph(Vec::new());
        let source = "Final long wrapped line with many constructs: bold words, inline code, a link, a bare URL https://example.com/final-check, and enough plain text to wrap several times in a narrow viewport.";
        let (segments, _) = wrap_facade_text_line(source, 78);

        let rendered = segments
            .into_iter()
            .map(|(start, end)| {
                line_text(&render_document_segment(&block, source, start, end, theme))
            })
            .collect::<Vec<_>>();
        let joined = rendered.join("\n");

        assert!(joined.contains("to wrap"));
        assert!(!joined.contains("to wra\nseveral"));
    }

    #[test]
    fn facade_list_wrapping_reports_marker_indent() {
        let (segments, marker_len) =
            wrap_facade_text_line("  ◦ Another nested bullet item that wraps for a while", 24);

        assert_eq!(marker_len, 4);
        assert!(segments.len() > 1);
        assert!(segments[1].0 >= marker_len);
    }

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }
}
