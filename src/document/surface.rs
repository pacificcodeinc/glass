use crate::document::model::{Block, Inline, inline_plain_text};
use crate::editor::render::word_wrap_segments;
use crate::markdown::highlight::concealed_wrap_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceMode {
    Inactive,
    Active { cursor_column: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceLine {
    pub text: String,
    pub source_map: Vec<Option<usize>>,
    source_to_display: Vec<usize>,
    pub marker_width: usize,
    pub editable: bool,
    revealed: bool,
}

impl SurfaceLine {
    pub fn plain(source: &str) -> Self {
        let mut builder = SurfaceBuilder::new(source.chars().count(), 0, true);
        builder.push_mapped(source, 0);
        builder.finish()
    }

    pub fn for_block(block: &Block, source: &str, mode: SurfaceMode) -> Self {
        let source_len = source.chars().count();
        let active_cursor = match mode {
            SurfaceMode::Inactive => None,
            SurfaceMode::Active { cursor_column } => Some(cursor_column),
        };

        match (block, active_cursor) {
            (Block::Heading { level, content }, Some(_)) => {
                let marker = format!("{} ", "#".repeat(*level as usize));
                let mut builder = SurfaceBuilder::new(source_len, 0, true);
                builder.push_virtual(&marker);
                builder.push_inlines(content, 0, Some(usize::MAX), true);
                builder.finish()
            }
            (Block::Quote { level, content }, Some(cursor)) => {
                let marker = format!("{} ", ">".repeat(*level as usize));
                let marker_len = marker.chars().count();
                let mut builder = SurfaceBuilder::new(source_len, marker_len, true);
                builder.revealed = true;
                builder.push_mapped(&marker, 0);
                builder.push_inlines(content, marker_len, cursor.checked_sub(marker_len), false);
                builder.finish()
            }
            (
                Block::ListItem {
                    indent,
                    marker,
                    content,
                },
                Some(cursor),
            ) => {
                let marker_text =
                    format!("{}{} ", " ".repeat(*indent), marker.plain_marker(*indent));
                let marker_len = marker_text.chars().count();
                let mut builder = SurfaceBuilder::new(source_len, marker_len, true);
                builder.push_mapped(&marker_text, 0);
                builder.push_inlines(content, marker_len, cursor.checked_sub(marker_len), false);
                builder.finish()
            }
            (
                Block::ChecklistItem {
                    indent,
                    checked,
                    content,
                },
                Some(cursor),
            ) => {
                let marker_text = format!(
                    "{}{} ",
                    " ".repeat(*indent),
                    if *checked { "[x]" } else { "[ ]" }
                );
                let marker_len = marker_text.chars().count();
                let mut builder = SurfaceBuilder::new(source_len, marker_len, true);
                builder.push_mapped(&marker_text, 0);
                builder.push_inlines(content, marker_len, cursor.checked_sub(marker_len), false);
                builder.finish()
            }
            (Block::Paragraph(content), Some(cursor)) => {
                let mut builder = SurfaceBuilder::new(source_len, 0, true);
                builder.push_inlines(content, 0, Some(cursor), false);
                builder.finish()
            }
            (Block::CodeFence { .. }, Some(_)) | (Block::RawMarkdown(_), Some(_)) => {
                Self::plain(source)
            }
            _ => {
                let marker_width = match block {
                    Block::ListItem { indent, marker, .. } => {
                        indent + marker.plain_marker(*indent).chars().count() + 1
                    }
                    Block::ChecklistItem { indent, .. } => indent + 4,
                    _ => 0,
                };
                let mut builder = SurfaceBuilder::new(source_len, marker_width, false);
                builder.push_mapped(source, 0);
                builder.finish()
            }
        }
    }

    pub fn display_len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn has_virtual_chars(&self) -> bool {
        self.source_map.iter().any(Option::is_none)
    }

    pub fn has_revealed_syntax(&self) -> bool {
        self.revealed || self.has_virtual_chars()
    }

    pub fn display_column_for_source_column(&self, source_column: usize) -> usize {
        self.source_to_display
            .get(source_column)
            .copied()
            .unwrap_or_else(|| self.text.chars().count())
    }

    pub fn source_column_for_display_column(&self, display_column: usize) -> usize {
        let display_len = self.text.chars().count();
        let source_len = self.source_to_display.len().saturating_sub(1);
        if display_column >= display_len {
            return source_len;
        }

        if let Some(Some(source)) = self.source_map.get(display_column) {
            return *source;
        }

        if let Some(source) = self
            .source_map
            .iter()
            .skip(display_column)
            .flatten()
            .copied()
            .next()
        {
            return source;
        }

        self.source_map
            .iter()
            .take(display_column.saturating_add(1))
            .rev()
            .flatten()
            .copied()
            .next()
            .unwrap_or(source_len)
    }

    pub fn wrap_source_segments(&self, width: usize) -> (Vec<(usize, usize)>, usize) {
        let display_segments = self.wrap_display_segments(width);
        let source_len = self.source_to_display.len().saturating_sub(1);
        let segments = display_segments
            .into_iter()
            .map(|(start, end)| {
                let source_start = self.source_column_for_display_boundary(start);
                let source_end = self
                    .source_column_for_display_boundary(end)
                    .max(source_start);
                (source_start.min(source_len), source_end.min(source_len))
            })
            .collect::<Vec<_>>();
        (segments, self.marker_width)
    }

    pub fn wrap_display_segments(&self, width: usize) -> Vec<(usize, usize)> {
        let width = width.max(1);
        if self.marker_width == 0 || self.marker_width >= width {
            return word_wrap_segments(&self.text, width);
        }

        let display_len = self.display_len();
        let content = char_slice(&self.text, self.marker_width, display_len);
        if content.is_empty() {
            return vec![(0, display_len)];
        }

        let content_width = width - self.marker_width;
        let content_segments = word_wrap_segments(&content, content_width);
        let mut segments = Vec::new();
        for (index, (start, end)) in content_segments.into_iter().enumerate() {
            if index == 0 {
                segments.push((0, self.marker_width + end));
            } else {
                segments.push((self.marker_width + start, self.marker_width + end));
            }
        }
        segments
    }

    pub fn source_map_for_display_range(&self, start: usize, end: usize) -> Vec<Option<usize>> {
        self.source_map
            .iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .copied()
            .collect()
    }

    fn source_column_for_display_boundary(&self, boundary: usize) -> usize {
        if boundary == 0 {
            return self.source_column_for_display_column(0);
        }
        if boundary >= self.text.chars().count() {
            return self.source_to_display.len().saturating_sub(1);
        }

        let left = self
            .source_map
            .get(boundary.saturating_sub(1))
            .copied()
            .flatten();
        let right = self.source_map.get(boundary).copied().flatten();
        match (left, right) {
            (Some(left), Some(right)) if right > left => right,
            (Some(left), _) => left.saturating_add(1),
            (_, Some(right)) => right,
            _ => self.source_column_for_display_column(boundary),
        }
    }
}

pub fn wrap_surface_or_facade_line(
    block: Option<&Block>,
    source: &str,
    width: usize,
    mode: SurfaceMode,
) -> (Vec<(usize, usize)>, usize) {
    match (block, mode) {
        (Some(block), SurfaceMode::Active { .. }) => {
            let surface = SurfaceLine::for_block(block, source, mode);
            if surface.has_revealed_syntax() {
                surface.wrap_source_segments(width)
            } else {
                concealed_wrap_line(source, width)
            }
        }
        _ => concealed_wrap_line(source, width),
    }
}

struct SurfaceBuilder {
    text: String,
    source_map: Vec<Option<usize>>,
    source_len: usize,
    marker_width: usize,
    editable: bool,
    revealed: bool,
}

impl SurfaceBuilder {
    fn new(source_len: usize, marker_width: usize, editable: bool) -> Self {
        Self {
            text: String::new(),
            source_map: Vec::new(),
            source_len,
            marker_width,
            editable,
            revealed: false,
        }
    }

    fn finish(self) -> SurfaceLine {
        let mut source_to_display = vec![usize::MAX; self.source_len.saturating_add(1)];
        for (display_index, source) in self.source_map.iter().copied().enumerate() {
            if let Some(source) = source
                && let Some(slot) = source_to_display.get_mut(source)
                && *slot == usize::MAX
            {
                *slot = display_index;
            }
        }

        let display_len = self.text.chars().count();
        source_to_display[self.source_len] = display_len;
        let mut next = display_len;
        for index in (0..source_to_display.len()).rev() {
            if source_to_display[index] == usize::MAX {
                source_to_display[index] = next;
            } else {
                next = source_to_display[index];
            }
        }

        SurfaceLine {
            text: self.text,
            source_map: self.source_map,
            source_to_display,
            marker_width: self.marker_width,
            editable: self.editable,
            revealed: self.revealed,
        }
    }

    fn push_virtual(&mut self, text: &str) {
        self.revealed = true;
        for ch in text.chars() {
            self.text.push(ch);
            self.source_map.push(None);
        }
    }

    fn push_mapped(&mut self, text: &str, source_start: usize) {
        for (offset, ch) in text.chars().enumerate() {
            self.text.push(ch);
            self.source_map.push(Some(source_start + offset));
        }
    }

    fn push_inlines(
        &mut self,
        inlines: &[Inline],
        source_start: usize,
        cursor_column: Option<usize>,
        reveal_all: bool,
    ) {
        let mut source_cursor = source_start;
        for inline in inlines {
            let plain = inline_plain_text(std::slice::from_ref(inline));
            let plain_len = plain.chars().count();
            let local_cursor = cursor_column.and_then(|cursor| {
                let inline_start = source_cursor.saturating_sub(source_start);
                (cursor >= inline_start && cursor <= inline_start + plain_len)
                    .then(|| cursor - inline_start)
            });
            let reveal = reveal_all || local_cursor.is_some();
            self.push_inline(inline, source_cursor, local_cursor, reveal);
            source_cursor += plain_len;
        }
    }

    fn push_inline(
        &mut self,
        inline: &Inline,
        source_start: usize,
        cursor_column: Option<usize>,
        reveal: bool,
    ) {
        match inline {
            Inline::Text(text) => self.push_mapped(text, source_start),
            Inline::BareUrl(text) => {
                if reveal {
                    self.revealed = true;
                }
                self.push_mapped(text, source_start);
            }
            Inline::Code(code) if reveal => {
                self.push_virtual("`");
                self.push_mapped(code, source_start);
                self.push_virtual("`");
            }
            Inline::Code(code) => self.push_mapped(code, source_start),
            Inline::Strong(children) if reveal => {
                self.push_virtual("**");
                self.push_inlines(children, source_start, cursor_column, true);
                self.push_virtual("**");
            }
            Inline::Strong(children) => {
                self.push_inlines(children, source_start, cursor_column, false)
            }
            Inline::Emphasis(children) if reveal => {
                self.push_virtual("*");
                self.push_inlines(children, source_start, cursor_column, true);
                self.push_virtual("*");
            }
            Inline::Emphasis(children) => {
                self.push_inlines(children, source_start, cursor_column, false)
            }
            Inline::Link {
                label,
                target,
                kind,
            } if reveal => match kind {
                crate::markdown::inline::LinkKind::Markdown => {
                    self.push_virtual("[");
                    self.push_inlines(label, source_start, cursor_column, true);
                    self.push_virtual("](");
                    self.push_virtual(target);
                    self.push_virtual(")");
                }
                crate::markdown::inline::LinkKind::Wiki => {
                    self.push_virtual("[[");
                    self.push_inlines(label, source_start, cursor_column, true);
                    self.push_virtual("]]");
                }
                crate::markdown::inline::LinkKind::Url => self.push_mapped(target, source_start),
            },
            Inline::Link {
                label,
                target,
                kind,
            } => {
                if label.is_empty() && !matches!(kind, crate::markdown::inline::LinkKind::Wiki) {
                    self.push_mapped(target, source_start);
                } else {
                    self.push_inlines(label, source_start, cursor_column, false);
                }
            }
        }
    }
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
    use crate::document::MarkdownCodec;

    #[test]
    fn active_heading_reveals_marker() {
        let document = MarkdownCodec::parse("# Heading");
        let block = document.block_for_plain_line(0).unwrap();
        let surface =
            SurfaceLine::for_block(block, "Heading", SurfaceMode::Active { cursor_column: 0 });

        assert_eq!(surface.text, "# Heading");
        assert_eq!(surface.display_column_for_source_column(0), 2);
    }

    #[test]
    fn active_link_reveals_markdown_when_cursor_is_inside_label() {
        let document = MarkdownCodec::parse("[README](README.md)");
        let block = document.block_for_plain_line(0).unwrap();
        let surface =
            SurfaceLine::for_block(block, "README", SurfaceMode::Active { cursor_column: 2 });

        assert_eq!(surface.text, "[README](README.md)");
        assert_eq!(surface.source_column_for_display_column(1), 0);
    }

    #[test]
    fn active_checklist_keeps_facade_marker() {
        let document = MarkdownCodec::parse("- [ ] task");
        let block = document.block_for_plain_line(0).unwrap();
        let surface =
            SurfaceLine::for_block(block, "[ ] task", SurfaceMode::Active { cursor_column: 0 });

        assert_eq!(surface.text, "[ ] task");
        assert!(!surface.text.starts_with("- [ ]"));
    }

    #[test]
    fn inactive_text_stays_plain() {
        let document = MarkdownCodec::parse("# Heading");
        let block = document.block_for_plain_line(0).unwrap();
        let surface = SurfaceLine::for_block(block, "Heading", SurfaceMode::Inactive);

        assert_eq!(surface.text, "Heading");
    }

    #[test]
    fn wrap_segments_map_back_to_source() {
        let document = MarkdownCodec::parse("# Heading with many words");
        let block = document.block_for_plain_line(0).unwrap();
        let surface = SurfaceLine::for_block(
            block,
            "Heading with many words",
            SurfaceMode::Active { cursor_column: 0 },
        );
        let (segments, _) = surface.wrap_source_segments(10);

        assert!(segments.len() > 1);
        assert_eq!(segments[0].0, 0);
    }

    #[test]
    fn active_list_wraps_with_marker_indent() {
        let document = MarkdownCodec::parse("- one two three four");
        let block = document.block_for_plain_line(0).unwrap();
        let surface = SurfaceLine::for_block(
            block,
            "• one two three four",
            SurfaceMode::Active { cursor_column: 4 },
        );
        let (segments, marker_width) = surface.wrap_source_segments(10);

        assert_eq!(marker_width, 2);
        assert!(segments.len() > 1);
        assert!(segments[1].0 >= marker_width);
    }
}
