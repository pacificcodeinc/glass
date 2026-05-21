use std::path::{Path, PathBuf};

use anyhow::Result;
use ropey::Rope;

use crate::{
    document::{
        Block, DocLink, DocRange, Document, Inline, MarkdownCodec, SurfaceLine, SurfaceMode,
        TableAlignment, TableCell, TableRow,
        markdown::{inline_plain_column_for_source_column, parse_inlines},
        model::{inline_markdown, inline_plain_text},
    },
    editor::{
        commands::{TableColumnPlacement, TableRowPlacement},
        cursor::Cursor,
    },
    fs::persistence,
    markdown::parse::parse_markdown,
};

#[derive(Debug, Clone)]
pub struct DocumentBuffer {
    pub path: Option<PathBuf>,
    document: Document,
    text: Rope,
    pub dirty: bool,
    saved_markdown: String,
    undo_stack: Vec<BufferSnapshot>,
}

#[derive(Debug, Clone)]
struct BufferSnapshot {
    document: Document,
    text: Rope,
    cursor: Cursor,
}

impl DocumentBuffer {
    pub fn empty() -> Self {
        let document = Document::default();
        let text = Rope::from_str(&document.plain_text());
        Self {
            path: None,
            document,
            text,
            dirty: false,
            saved_markdown: String::new(),
            undo_stack: Vec::new(),
        }
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let contents = persistence::load_utf8(path)?;
        parse_markdown(&contents)?;
        let document = MarkdownCodec::parse(&contents);
        let saved_markdown = MarkdownCodec::serialize(&document);
        let text = Rope::from_str(&document.plain_text());
        Ok(Self {
            path: Some(path.to_path_buf()),
            document,
            text,
            dirty: false,
            saved_markdown,
            undo_stack: Vec::new(),
        })
    }

    pub fn from_path_or_empty(path: &Path) -> Result<Self> {
        if path.exists() {
            Self::from_path(path)
        } else {
            Ok(Self {
                path: Some(path.to_path_buf()),
                document: Document::default(),
                text: Rope::new(),
                dirty: false,
                saved_markdown: String::new(),
                undo_stack: Vec::new(),
            })
        }
    }

    pub fn save(&mut self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };

        let markdown = self.markdown_string();
        persistence::save_atomic(path, &markdown)?;
        self.saved_markdown = markdown;
        self.dirty = false;
        Ok(())
    }

    pub fn line_count(&self) -> usize {
        self.visible_len_lines()
    }

    pub fn line(&self, line: usize) -> String {
        if line >= self.text.len_lines() {
            return String::new();
        }

        self.text.line(line).to_string()
    }

    pub fn line_len_chars(&self, line: usize) -> usize {
        trim_line_ending_len(&self.line(line))
    }

    pub fn char_index(&self, cursor: Cursor) -> usize {
        let line = cursor.line.min(self.text.len_lines().saturating_sub(1));
        let line_start = self.text.line_to_char(line);
        line_start + cursor.column.min(self.line_len_chars(line))
    }

    pub fn insert_char(&mut self, cursor: &mut Cursor, ch: char) {
        self.push_undo_snapshot(*cursor);
        self.insert_char_raw(cursor, ch);
    }

    fn insert_char_raw(&mut self, cursor: &mut Cursor, ch: char) {
        let index = self.char_index(*cursor);
        self.text.insert_char(index, ch);

        if ch == '\n' {
            cursor.line += 1;
            cursor.column = 0;
        } else {
            cursor.column += 1;
        }
        self.sync_document_from_facade(cursor);
    }

    pub fn insert_str(&mut self, cursor: &mut Cursor, value: &str) {
        if value.is_empty() {
            return;
        }

        self.push_undo_snapshot(*cursor);
        let index = self.char_index(*cursor);
        self.text.insert(index, value);
        for ch in value.chars() {
            if ch == '\n' {
                cursor.line += 1;
                cursor.column = 0;
            } else {
                cursor.column += 1;
            }
        }
        self.sync_document_from_facade(cursor);
    }

    pub fn delete_previous_char(&mut self, cursor: &mut Cursor) {
        if cursor.line == 0 && cursor.column == 0 {
            return;
        }

        let end = self.char_index(*cursor);
        if end == 0 {
            return;
        }

        self.push_undo_snapshot(*cursor);
        let previous = end - 1;
        let previous_line_len = if cursor.column == 0 {
            Some(self.line_len_chars(cursor.line.saturating_sub(1)))
        } else {
            None
        };
        self.text.remove(previous..end);

        if cursor.column > 0 {
            cursor.column -= 1;
        } else {
            cursor.line = cursor.line.saturating_sub(1);
            cursor.column = previous_line_len.unwrap_or_default();
        }
        self.sync_document_from_facade(cursor);
    }

    pub fn delete_char(&mut self, cursor: &mut Cursor) {
        let start = self.char_index(*cursor);
        if start >= self.text.len_chars() {
            return;
        }

        self.push_undo_snapshot(*cursor);
        self.text.remove(start..start + 1);
        self.sync_document_from_facade(cursor);
        self.clamp_cursor(cursor);
    }

    pub fn delete_range(&mut self, start: usize, end: usize, cursor: &mut Cursor) {
        self.delete_range_impl(start, end, cursor, true);
    }

    fn delete_range_impl(
        &mut self,
        start: usize,
        end: usize,
        cursor: &mut Cursor,
        record_undo: bool,
    ) {
        let start = start.min(self.text.len_chars());
        let end = end.min(self.text.len_chars());
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        if start == end {
            return;
        }

        if record_undo {
            self.push_undo_snapshot(*cursor);
        }
        self.text.remove(start..end);
        *cursor = self.cursor_from_char_index(start);
        self.sync_document_from_facade(cursor);
        self.clamp_cursor(cursor);
    }

    pub fn replace_range(
        &mut self,
        start: usize,
        end: usize,
        replacement: &str,
        cursor: &mut Cursor,
    ) {
        let start = start.min(self.text.len_chars());
        let end = end.min(self.text.len_chars());
        let (start, end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        if start == end && replacement.is_empty() {
            return;
        }

        self.push_undo_snapshot(*cursor);
        if start != end {
            self.text.remove(start..end);
        }
        if !replacement.is_empty() {
            self.text.insert(start, replacement);
        }
        *cursor = self.cursor_from_char_index(start + replacement.chars().count());
        self.sync_document_from_facade(cursor);
        self.clamp_cursor(cursor);
    }

    pub fn delete_line_range(&mut self, start_line: usize, end_line: usize, cursor: &mut Cursor) {
        let target_column = cursor.column;
        let start_line = start_line.min(self.line_count().saturating_sub(1));
        let end_line = end_line.min(self.line_count().saturating_sub(1));
        let (start_line, end_line) = if start_line <= end_line {
            (start_line, end_line)
        } else {
            (end_line, start_line)
        };

        let start = self.line_start_char_index(start_line);
        let end = if end_line + 1 < self.text.len_lines() {
            self.line_start_char_index(end_line + 1)
        } else {
            self.text.len_chars()
        };

        self.delete_range(start, end, cursor);
        cursor.line = start_line.min(self.line_count().saturating_sub(1));
        cursor.column = target_column;
        self.clamp_cursor(cursor);
    }

    pub fn line_start_char_index(&self, line: usize) -> usize {
        let line = line.min(self.text.len_lines().saturating_sub(1));
        self.text.line_to_char(line)
    }

    pub fn cursor_from_char_index(&self, index: usize) -> Cursor {
        let index = index.min(self.text.len_chars());
        let line = self.text.char_to_line(index);
        let column = index.saturating_sub(self.text.line_to_char(line));
        let mut cursor = Cursor { line, column };
        self.clamp_cursor(&mut cursor);
        cursor
    }

    pub fn clamp_cursor(&self, cursor: &mut Cursor) {
        cursor.line = cursor.line.min(self.line_count().saturating_sub(1));
        cursor.column = cursor.column.min(self.line_len_chars(cursor.line));
    }

    pub fn as_string(&self) -> String {
        self.text.to_string()
    }

    pub fn markdown_string(&self) -> String {
        MarkdownCodec::serialize(&self.document)
    }

    pub fn selected_markdown(&self, start: Cursor, end: Cursor) -> Option<String> {
        let start = self.char_index(start);
        let end = self.char_index(end);
        if start == end {
            return None;
        }

        Some(self.document.serialize_range(DocRange { start, end }))
    }

    pub fn link_at_cursor(&self, cursor: Cursor) -> Option<DocLink> {
        self.document
            .link_at_plain_position(cursor.line, cursor.column)
    }

    pub fn block_for_line(&self, line: usize) -> Option<&Block> {
        self.document.block_for_plain_line(line)
    }

    pub fn surface_line(&self, line: usize, mode: SurfaceMode) -> SurfaceLine {
        let source = self.line(line);
        let source = source.trim_end_matches(['\r', '\n']);
        self.block_for_line(line)
            .map(|block| SurfaceLine::for_block(block, source, mode))
            .unwrap_or_else(|| SurfaceLine::plain(source))
    }

    pub fn replace_line_from_surface(
        &mut self,
        line: usize,
        surface_text: &str,
        surface_column: usize,
        cursor: &mut Cursor,
    ) -> usize {
        let line = line.min(self.line_count().saturating_sub(1));
        let block_index = self.block_index_for_line(line);
        let parsed = MarkdownCodec::parse_plain(surface_text);
        let replacement = parsed.blocks.into_iter().next().unwrap_or(Block::Blank);

        self.push_undo_snapshot(*cursor);
        if let Some(block) = self.document.blocks.get_mut(block_index) {
            *block = replacement;
        } else {
            self.document.blocks.push(replacement);
        }

        self.text = Rope::from_str(&self.document.plain_text());
        cursor.line = line.min(self.line_count().saturating_sub(1));
        let active_surface = self.surface_line(
            cursor.line,
            SurfaceMode::Active {
                cursor_column: self.line_len_chars(cursor.line),
            },
        );
        let display_column = surface_column.min(active_surface.display_len());
        cursor.column = active_surface
            .source_column_for_display_column(display_column)
            .min(self.line_len_chars(cursor.line));
        self.update_dirty();
        display_column
    }

    pub fn toggle_checkbox_at_line(&mut self, line: usize, cursor: &mut Cursor) -> bool {
        let mut plain_line = 0usize;
        for index in 0..self.document.blocks.len() {
            let block = &self.document.blocks[index];
            let line_count = block.plain_line_count();
            if line >= plain_line && line < plain_line + line_count {
                if matches!(block, Block::ChecklistItem { .. }) {
                    self.push_undo_snapshot(*cursor);
                    let Block::ChecklistItem { checked, .. } = &mut self.document.blocks[index]
                    else {
                        unreachable!();
                    };
                    *checked = !*checked;
                    self.rebuild_facade_preserving_cursor(cursor);
                    self.update_dirty();
                    return true;
                }
                return false;
            }
            plain_line += line_count;
        }
        false
    }

    pub fn delete_structural_list_marker_at_cursor(
        &mut self,
        cursor: &mut Cursor,
        include_content_boundary: bool,
    ) -> bool {
        let line = cursor.line.min(self.line_count().saturating_sub(1));
        let block_index = self.block_index_for_line(line);
        let Some(block) = self.document.blocks.get(block_index) else {
            return false;
        };

        let Some((marker_width, content)) = structural_list_marker(block) else {
            return false;
        };
        let content_is_empty = inline_plain_text(&content).is_empty();
        let cursor_is_in_marker = cursor.column < marker_width
            || (cursor.column == marker_width && (include_content_boundary || content_is_empty));
        if !cursor_is_in_marker {
            return false;
        }

        self.push_undo_snapshot(*cursor);
        let replacement = if content_is_empty {
            Block::Blank
        } else {
            Block::Paragraph(content)
        };
        if let Some(block) = self.document.blocks.get_mut(block_index) {
            *block = replacement;
        }

        self.text = Rope::from_str(&self.document.plain_text());
        cursor.line = line.min(self.line_count().saturating_sub(1));
        cursor.column = 0;
        self.update_dirty();
        true
    }

    pub fn insert_table(&mut self, rows: usize, columns: usize, cursor: &mut Cursor) {
        self.push_undo_snapshot(*cursor);
        let rows = rows.max(2);
        let columns = columns.max(2);
        let mut table_rows = Vec::new();
        for row in 0..rows {
            table_rows.push(TableRow {
                cells: (0..columns)
                    .map(|column| TableCell {
                        content: vec![Inline::Text(if row == 0 {
                            format!("Column {}", column + 1)
                        } else {
                            String::new()
                        })],
                    })
                    .collect(),
            });
        }

        let insert_at = self.block_index_for_line(cursor.line);
        self.document.blocks.insert(
            insert_at,
            Block::Table {
                alignments: vec![TableAlignment::Left; columns],
                rows: table_rows,
            },
        );
        self.rebuild_facade_preserving_cursor(cursor);
        self.update_dirty();
    }

    pub fn insert_table_row_at_cursor(
        &mut self,
        cursor: &mut Cursor,
        placement: TableRowPlacement,
    ) -> bool {
        let Some(location) = self.table_location_at_cursor(*cursor) else {
            return false;
        };

        self.push_undo_snapshot(*cursor);
        let Block::Table { rows, .. } = &mut self.document.blocks[location.block_index] else {
            return false;
        };

        let column_count = rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(location.column_index + 1)
            .max(1);
        let insert_at = match placement {
            TableRowPlacement::Above => location.row_index,
            TableRowPlacement::Below => location.row_index + 1,
        }
        .min(rows.len());
        rows.insert(
            insert_at,
            TableRow {
                cells: empty_table_cells(column_count),
            },
        );

        self.rebuild_facade_preserving_cursor(cursor);
        cursor.line = (location.block_start + table_source_line_for_row(insert_at))
            .min(self.line_count().saturating_sub(1));
        cursor.column = table_cell_ranges(&self.line(cursor.line))
            .get(location.column_index.min(column_count.saturating_sub(1)))
            .map(|(start, _)| *start)
            .unwrap_or_default();
        self.update_dirty();
        true
    }

    pub fn insert_table_column_at_cursor(
        &mut self,
        cursor: &mut Cursor,
        placement: TableColumnPlacement,
    ) -> bool {
        let Some(location) = self.table_location_at_cursor(*cursor) else {
            return false;
        };

        self.push_undo_snapshot(*cursor);
        let Block::Table { alignments, rows } = &mut self.document.blocks[location.block_index]
        else {
            return false;
        };

        let column_count = rows.iter().map(|row| row.cells.len()).max().unwrap_or(0);
        let insert_at = match placement {
            TableColumnPlacement::Left => location.column_index,
            TableColumnPlacement::Right => location.column_index + 1,
        }
        .min(column_count);

        for (row_index, row) in rows.iter_mut().enumerate() {
            while row.cells.len() < column_count {
                row.cells.push(empty_table_cell());
            }
            row.cells.insert(
                insert_at,
                TableCell {
                    content: vec![Inline::Text(if row_index == 0 {
                        format!("Column {}", insert_at + 1)
                    } else {
                        String::new()
                    })],
                },
            );
        }
        alignments.resize(column_count, TableAlignment::Left);
        alignments.insert(insert_at, TableAlignment::Left);

        self.rebuild_facade_preserving_cursor(cursor);
        cursor.line = (location.block_start + table_source_line_for_row(location.row_index))
            .min(self.line_count().saturating_sub(1));
        cursor.column = table_cell_ranges(&self.line(cursor.line))
            .get(insert_at)
            .map(|(start, _)| *start)
            .unwrap_or_default();
        self.update_dirty();
        true
    }

    pub fn enter_table_cell(&mut self, cursor: &mut Cursor) -> bool {
        let Some(location) = self.table_location_at_cursor(*cursor) else {
            return false;
        };
        let Some(Block::Table { rows, .. }) = self.document.blocks.get(location.block_index) else {
            return false;
        };

        if location.row_index + 1 >= rows.len() {
            return self.insert_table_row_at_cursor(cursor, TableRowPlacement::Below);
        }

        cursor.line = (location.block_start + table_source_line_for_row(location.row_index + 1))
            .min(self.line_count().saturating_sub(1));
        cursor.column = table_cell_ranges(&self.line(cursor.line))
            .get(location.column_index)
            .map(|(start, _)| *start)
            .unwrap_or_default();
        true
    }

    pub fn insert_table_char(&mut self, cursor: &mut Cursor, ch: char) -> bool {
        if ch == '\n' {
            return false;
        }
        let Some(location) = self.table_edit_location_at_cursor(*cursor) else {
            return self.cursor_is_inside_table_block(*cursor);
        };

        let Some(mut text) = self.table_cell_markdown(location) else {
            return self.cursor_is_inside_table_block(*cursor);
        };
        self.push_undo_snapshot(*cursor);
        insert_char_at_column(&mut text, location.cell_column, ch);
        let Some(cell) = self.table_cell_mut(location) else {
            return self.cursor_is_inside_table_block(*cursor);
        };
        cell.content = parse_inlines(&text);

        self.rebuild_facade_preserving_cursor(cursor);
        *cursor = self.cursor_for_table_cell(location, location.cell_column + 1);
        self.update_dirty();
        true
    }

    pub fn delete_table_char(&mut self, cursor: &mut Cursor, backspace: bool) -> bool {
        let Some(location) = self.table_edit_location_at_cursor(*cursor) else {
            return self.cursor_is_inside_table_block(*cursor);
        };

        let Some(cell_text) = self.table_cell_markdown(location) else {
            return self.cursor_is_inside_table_block(*cursor);
        };
        let text_len = cell_text.chars().count();
        let delete_column = if backspace {
            let Some(column) = location.cell_column.checked_sub(1) else {
                return true;
            };
            column
        } else if location.cell_column < text_len {
            location.cell_column
        } else {
            return true;
        };

        self.push_undo_snapshot(*cursor);
        let mut text = cell_text;
        remove_char_at_column(&mut text, delete_column);
        let Some(cell) = self.table_cell_mut(location) else {
            return self.cursor_is_inside_table_block(*cursor);
        };
        cell.content = parse_inlines(&text);

        self.rebuild_facade_preserving_cursor(cursor);
        *cursor = self.cursor_for_table_cell(location, delete_column);
        self.update_dirty();
        true
    }

    pub fn move_table_cell(&self, cursor: &mut Cursor, delta: isize) -> bool {
        if delta == 0 || !is_table_content_line(&self.line(cursor.line)) {
            return false;
        }

        let line = self.line(cursor.line);
        let cells = table_cell_ranges(&line);
        if cells.len() < 2 {
            return false;
        }

        let current = cells
            .iter()
            .position(|(start, end)| cursor.column >= *start && cursor.column <= *end)
            .unwrap_or_else(|| {
                cells
                    .iter()
                    .position(|(start, _)| cursor.column < *start)
                    .unwrap_or(cells.len() - 1)
            });
        let target = current as isize + delta;
        if target >= 0 && (target as usize) < cells.len() {
            cursor.column = cells[target as usize].0;
            return true;
        }

        let mut next_line = cursor.line;
        loop {
            next_line = if delta > 0 {
                next_line + 1
            } else {
                next_line.saturating_sub(1)
            };
            if next_line == cursor.line || next_line >= self.line_count() {
                return false;
            }
            if is_table_content_line(&self.line(next_line)) {
                break;
            }
            if !self.line(next_line).contains('|') {
                return false;
            }
        }

        let next_text = self.line(next_line);
        if !is_table_content_line(&next_text) {
            return false;
        }
        let next_cells = table_cell_ranges(&next_text);
        if next_cells.is_empty() {
            return false;
        }

        cursor.line = next_line;
        cursor.column = if delta > 0 {
            next_cells[0].0
        } else {
            next_cells[next_cells.len() - 1].0
        };
        true
    }

    pub fn undo(&mut self, cursor: &mut Cursor) -> bool {
        let Some(snapshot) = self.undo_stack.pop() else {
            return false;
        };

        self.document = snapshot.document;
        self.text = snapshot.text;
        *cursor = snapshot.cursor;
        self.update_dirty();
        self.clamp_cursor(cursor);
        true
    }

    fn push_undo_snapshot(&mut self, cursor: Cursor) {
        if self
            .undo_stack
            .last()
            .is_some_and(|snapshot| snapshot.text == self.text && snapshot.cursor == cursor)
        {
            return;
        }

        self.undo_stack.push(BufferSnapshot {
            document: self.document.clone(),
            text: self.text.clone(),
            cursor,
        });
    }

    fn update_dirty(&mut self) {
        self.dirty = self.markdown_string() != self.saved_markdown;
    }

    fn sync_document_from_facade(&mut self, cursor: &mut Cursor) {
        let old_document = self.document.clone();
        let source_line = self
            .line(cursor.line)
            .trim_end_matches(['\r', '\n'])
            .to_string();
        let source_column = cursor.column;
        let parsed = MarkdownCodec::parse_plain(&self.text.to_string());
        self.document = reconcile_facade_document(&old_document, parsed);
        self.rebuild_facade_after_parse(cursor, &source_line, source_column);
        self.update_dirty();
    }

    fn rebuild_facade_after_parse(
        &mut self,
        cursor: &mut Cursor,
        source_line: &str,
        source_column: usize,
    ) {
        let line = cursor.line;
        self.text = Rope::from_str(&self.document.plain_text());
        cursor.line = line.min(self.text.len_lines().saturating_sub(1));
        cursor.column = self
            .document
            .block_for_plain_line(cursor.line)
            .map(|block| remap_facade_cursor_after_parse(source_line, source_column, block))
            .unwrap_or(source_column)
            .min(self.line_len_chars(cursor.line));
    }

    fn rebuild_facade_preserving_cursor(&mut self, cursor: &mut Cursor) {
        let line = cursor.line;
        let column = cursor.column;
        self.text = Rope::from_str(&self.document.plain_text());
        cursor.line = line.min(self.text.len_lines().saturating_sub(1));
        cursor.column = column.min(self.line_len_chars(cursor.line));
    }

    fn block_index_for_line(&self, line: usize) -> usize {
        let mut plain_line = 0usize;
        for (index, block) in self.document.blocks.iter().enumerate() {
            let next = plain_line + block.plain_line_count();
            if line <= plain_line || line < next {
                return index;
            }
            plain_line = next;
        }
        self.document.blocks.len()
    }

    fn table_location_at_cursor(&self, cursor: Cursor) -> Option<TableCursorLocation> {
        let mut plain_line = 0usize;
        for (block_index, block) in self.document.blocks.iter().enumerate() {
            let line_count = block.plain_line_count();
            let next = plain_line + line_count;
            if cursor.line >= plain_line && cursor.line < next {
                let Block::Table { rows, .. } = block else {
                    return None;
                };
                let local_line = cursor.line - plain_line;
                let row_index =
                    table_row_for_source_line(local_line).min(rows.len().saturating_sub(1));
                let column_index = table_cell_ranges(&self.line(cursor.line))
                    .iter()
                    .position(|(start, end)| cursor.column >= *start && cursor.column <= *end)
                    .unwrap_or_else(|| {
                        table_cell_ranges(&self.line(cursor.line))
                            .iter()
                            .position(|(start, _)| cursor.column < *start)
                            .unwrap_or_else(|| {
                                rows.get(row_index)
                                    .map(|row| row.cells.len().saturating_sub(1))
                                    .unwrap_or_default()
                            })
                    });
                return Some(TableCursorLocation {
                    block_index,
                    block_start: plain_line,
                    row_index,
                    column_index,
                });
            }
            plain_line = next;
        }
        None
    }

    fn table_edit_location_at_cursor(&self, cursor: Cursor) -> Option<TableEditLocation> {
        let mut plain_line = 0usize;
        for (block_index, block) in self.document.blocks.iter().enumerate() {
            let line_count = block.plain_line_count();
            let next = plain_line + line_count;
            if cursor.line >= plain_line && cursor.line < next {
                let Block::Table { rows, .. } = block else {
                    return None;
                };
                let local_line = cursor.line - plain_line;
                if local_line == 1 {
                    return None;
                }
                let row_index =
                    table_row_for_source_line(local_line).min(rows.len().saturating_sub(1));
                let ranges = table_cell_ranges(&self.line(cursor.line));
                let cell_count = rows
                    .get(row_index)
                    .map(|row| row.cells.len())
                    .unwrap_or_default()
                    .max(ranges.len());
                if cell_count == 0 {
                    return None;
                }
                let column_index =
                    table_column_for_source_column(&ranges, cursor.column, cell_count);
                let (cell_start, cell_end) =
                    ranges.get(column_index).copied().unwrap_or_else(|| {
                        let line_len = self.line_len_chars(cursor.line);
                        (line_len, line_len)
                    });
                let cell_column = cursor
                    .column
                    .saturating_sub(cell_start)
                    .min(cell_end.saturating_sub(cell_start));
                return Some(TableEditLocation {
                    block_index,
                    block_start: plain_line,
                    row_index,
                    column_index,
                    cell_column,
                });
            }
            plain_line = next;
        }
        None
    }

    fn cursor_is_inside_table_block(&self, cursor: Cursor) -> bool {
        let mut plain_line = 0usize;
        for block in &self.document.blocks {
            let line_count = block.plain_line_count();
            let next = plain_line + line_count;
            if cursor.line >= plain_line && cursor.line < next {
                return matches!(block, Block::Table { .. });
            }
            plain_line = next;
        }
        false
    }

    fn table_cell_markdown(&self, location: TableEditLocation) -> Option<String> {
        let Block::Table { rows, .. } = self.document.blocks.get(location.block_index)? else {
            return None;
        };
        let cell = rows
            .get(location.row_index)?
            .cells
            .get(location.column_index)?;
        Some(inline_markdown(&cell.content))
    }

    fn table_cell_mut(&mut self, location: TableEditLocation) -> Option<&mut TableCell> {
        let Block::Table { rows, .. } = self.document.blocks.get_mut(location.block_index)? else {
            return None;
        };
        let column_count = rows
            .iter()
            .map(|row| row.cells.len())
            .max()
            .unwrap_or(location.column_index + 1)
            .max(location.column_index + 1);
        let row = rows.get_mut(location.row_index)?;
        while row.cells.len() < column_count {
            row.cells.push(empty_table_cell());
        }
        row.cells.get_mut(location.column_index)
    }

    fn cursor_for_table_cell(&self, location: TableEditLocation, cell_column: usize) -> Cursor {
        let line = (location.block_start + table_source_line_for_row(location.row_index))
            .min(self.line_count().saturating_sub(1));
        let column = table_cell_ranges(&self.line(line))
            .get(location.column_index)
            .map(|(start, end)| start + cell_column.min(end.saturating_sub(*start)))
            .unwrap_or_else(|| self.line_len_chars(line));
        Cursor { line, column }
    }

    fn visible_len_lines(&self) -> usize {
        let len_chars = self.text.len_chars();
        if len_chars == 0 {
            return 1;
        }

        let len_lines = self.text.len_lines();
        if self.text.char(len_chars - 1) == '\n' {
            len_lines.saturating_sub(1).max(1)
        } else {
            len_lines.max(1)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TableCursorLocation {
    block_index: usize,
    block_start: usize,
    row_index: usize,
    column_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TableEditLocation {
    block_index: usize,
    block_start: usize,
    row_index: usize,
    column_index: usize,
    cell_column: usize,
}

fn empty_table_cell() -> TableCell {
    TableCell {
        content: vec![Inline::Text(String::new())],
    }
}

fn empty_table_cells(count: usize) -> Vec<TableCell> {
    (0..count).map(|_| empty_table_cell()).collect()
}

fn table_row_for_source_line(local_line: usize) -> usize {
    match local_line {
        0 | 1 => 0,
        line => line - 1,
    }
}

fn table_source_line_for_row(row_index: usize) -> usize {
    if row_index == 0 { 0 } else { row_index + 1 }
}

fn table_column_for_source_column(
    ranges: &[(usize, usize)],
    column: usize,
    cell_count: usize,
) -> usize {
    ranges
        .iter()
        .position(|(start, end)| column >= *start && column <= *end)
        .or_else(|| ranges.iter().position(|(start, _)| column < *start))
        .unwrap_or_else(|| ranges.len().saturating_sub(1))
        .min(cell_count.saturating_sub(1))
}

fn insert_char_at_column(text: &mut String, column: usize, ch: char) {
    let byte_index = byte_index_for_char_column(text, column);
    text.insert(byte_index, ch);
}

fn remove_char_at_column(text: &mut String, column: usize) {
    let start = byte_index_for_char_column(text, column);
    let end = byte_index_for_char_column(text, column + 1);
    if start < end {
        text.replace_range(start..end, "");
    }
}

fn byte_index_for_char_column(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn reconcile_facade_document(old: &Document, parsed: Document) -> Document {
    if old.blocks.len() != parsed.blocks.len() {
        return parsed;
    }

    let blocks = parsed
        .blocks
        .into_iter()
        .enumerate()
        .map(|(index, parsed_block)| {
            let Some(old_block) = old.blocks.get(index) else {
                return parsed_block;
            };
            reconcile_block(old_block, parsed_block)
        })
        .collect();

    Document { blocks }
}

fn reconcile_block(old: &Block, parsed: Block) -> Block {
    match (&old, parsed) {
        (_, semantic @ Block::Heading { .. })
        | (_, semantic @ Block::Quote { .. })
        | (_, semantic @ Block::ListItem { .. })
        | (_, semantic @ Block::ChecklistItem { .. })
        | (_, semantic @ Block::CodeFence { .. })
        | (_, semantic @ Block::Table { .. })
        | (_, semantic @ Block::RawMarkdown(_)) => semantic,
        (Block::Heading { level, .. }, Block::Paragraph(content)) => Block::Heading {
            level: *level,
            content,
        },
        (Block::Quote { level, .. }, Block::Paragraph(content)) => Block::Quote {
            level: *level,
            content,
        },
        (Block::ListItem { indent, marker, .. }, Block::Paragraph(content)) => Block::ListItem {
            indent: *indent,
            marker: *marker,
            content,
        },
        (
            Block::ChecklistItem {
                indent, checked, ..
            },
            Block::Paragraph(content),
        ) => Block::ChecklistItem {
            indent: *indent,
            checked: *checked,
            content,
        },
        (_, block) => block,
    }
}

fn structural_list_marker(block: &Block) -> Option<(usize, Vec<Inline>)> {
    match block {
        Block::ListItem {
            indent,
            marker,
            content,
        } => Some((
            *indent + marker.plain_marker(*indent).chars().count() + 1,
            content.clone(),
        )),
        Block::ChecklistItem {
            indent, content, ..
        } => Some((*indent + 4, content.clone())),
        _ => None,
    }
}

fn remap_facade_cursor_after_parse(
    source_line: &str,
    source_column: usize,
    block: &Block,
) -> usize {
    match block {
        Block::Heading { level, .. } => {
            let Some(source_content_start) = heading_content_start(source_line, *level) else {
                return inline_plain_column_for_source_column(source_line, source_column);
            };
            if source_column <= source_content_start {
                return 0;
            }
            inline_plain_column_for_source_column(
                char_slice_from(source_line, source_content_start),
                source_column.saturating_sub(source_content_start),
            )
        }
        Block::Quote { level, .. } => {
            let Some((source_content_start, output_content_start)) =
                quote_content_columns(source_line, *level)
            else {
                return inline_plain_column_for_source_column(source_line, source_column);
            };
            if source_column <= source_content_start {
                return source_column.min(output_content_start);
            }
            output_content_start
                + inline_plain_column_for_source_column(
                    char_slice_from(source_line, source_content_start),
                    source_column.saturating_sub(source_content_start),
                )
        }
        Block::ListItem { indent, marker, .. } => {
            let Some((source_content_start, output_content_start)) = list_content_columns(
                source_line,
                *indent,
                marker.plain_marker(*indent).chars().count() + 1,
            ) else {
                return inline_plain_column_for_source_column(source_line, source_column);
            };
            if source_column <= source_content_start {
                return source_column.min(output_content_start);
            }
            output_content_start
                + inline_plain_column_for_source_column(
                    char_slice_from(source_line, source_content_start),
                    source_column.saturating_sub(source_content_start),
                )
        }
        Block::ChecklistItem { indent, .. } => {
            let Some((source_content_start, output_content_start)) =
                checklist_content_columns(source_line, *indent)
            else {
                return inline_plain_column_for_source_column(source_line, source_column);
            };
            if source_column <= source_content_start {
                let removed = source_content_start.saturating_sub(output_content_start);
                return source_column
                    .saturating_sub(removed)
                    .min(output_content_start);
            }
            output_content_start
                + inline_plain_column_for_source_column(
                    char_slice_from(source_line, source_content_start),
                    source_column.saturating_sub(source_content_start),
                )
        }
        Block::Paragraph(_) => inline_plain_column_for_source_column(source_line, source_column),
        _ => source_column,
    }
}

fn heading_content_start(source_line: &str, level: u8) -> Option<usize> {
    if leading_whitespace_chars(source_line) != 0 {
        return None;
    }
    let marker_len = level as usize + 1;
    let marker = format!("{} ", "#".repeat(level as usize));
    source_line.starts_with(&marker).then_some(marker_len)
}

fn quote_content_columns(source_line: &str, level: u8) -> Option<(usize, usize)> {
    let leading = leading_whitespace_chars(source_line);
    let trimmed = source_line.trim_start();
    let quote_len = trimmed.chars().take_while(|ch| *ch == '>').count();
    if quote_len == 0 || quote_len != level as usize {
        return None;
    }
    let spaces = trimmed
        .chars()
        .skip(quote_len)
        .take_while(|ch| ch.is_whitespace())
        .count();
    Some((leading + quote_len + spaces, leading + quote_len + 1))
}

fn list_content_columns(
    source_line: &str,
    indent: usize,
    output_marker_len: usize,
) -> Option<(usize, usize)> {
    let leading = leading_whitespace_chars(source_line);
    let trimmed = source_line.trim_start();
    let source_marker_len = bullet_marker_len(trimmed).or_else(|| ordered_marker_len(trimmed))?;
    Some((leading + source_marker_len, indent + output_marker_len))
}

fn checklist_content_columns(source_line: &str, indent: usize) -> Option<(usize, usize)> {
    let leading = leading_whitespace_chars(source_line);
    let trimmed = source_line.trim_start();
    let source_marker_len = marker_len(
        trimmed,
        &[
            "- [ ] ", "- [x] ", "[ ] ", "[x] ", "• [ ] ", "• [x] ", "◦ [ ] ", "◦ [x] ",
        ],
    )?;
    Some((leading + source_marker_len, indent + 4))
}

fn bullet_marker_len(trimmed: &str) -> Option<usize> {
    marker_len(trimmed, &["- ", "* ", "+ ", "• ", "◦ "])
}

fn ordered_marker_len(trimmed: &str) -> Option<usize> {
    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    (index > 0 && trimmed.get(index..index + 2) == Some(". ")).then_some(index + 2)
}

fn marker_len(source: &str, markers: &[&str]) -> Option<usize> {
    markers
        .iter()
        .find(|marker| source.starts_with(**marker))
        .map(|marker| marker.chars().count())
}

fn leading_whitespace_chars(source: &str) -> usize {
    source.chars().take_while(|ch| ch.is_whitespace()).count()
}

fn char_slice_from(source: &str, start: usize) -> &str {
    let byte_index = source
        .char_indices()
        .nth(start)
        .map(|(index, _)| index)
        .unwrap_or(source.len());
    &source[byte_index..]
}

fn trim_line_ending_len(line: &str) -> usize {
    line.trim_end_matches(['\r', '\n']).chars().count()
}

fn is_table_content_line(line: &str) -> bool {
    let trimmed = line.trim_end_matches(['\r', '\n']).trim();
    if !trimmed.contains('|') || is_table_delimiter_line(trimmed) {
        return false;
    }
    table_cell_ranges(trimmed).len() >= 2
}

fn is_table_delimiter_line(line: &str) -> bool {
    let cells = line
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect::<Vec<_>>();
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let dashes = cell.trim_matches(':');
            dashes.len() >= 3 && dashes.chars().all(|ch| ch == '-')
        })
}

fn table_cell_ranges(line: &str) -> Vec<(usize, usize)> {
    let chars = line
        .trim_end_matches(['\r', '\n'])
        .chars()
        .collect::<Vec<_>>();
    let pipes = chars
        .iter()
        .enumerate()
        .filter_map(|(index, ch)| (*ch == '|').then_some(index))
        .collect::<Vec<_>>();
    if pipes.len() < 2 {
        return Vec::new();
    }

    pipes
        .windows(2)
        .filter_map(|window| {
            let mut start = window[0] + 1;
            let mut end = window[1];
            while start < end && chars[start].is_whitespace() {
                start += 1;
            }
            while end > start && chars[end - 1].is_whitespace() {
                end -= 1;
            }
            Some((start, end))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_text_tracks_cursor() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();

        buffer.insert_str(&mut cursor, "a\nb");

        assert_eq!(buffer.as_string(), "a\nb");
        assert_eq!(cursor.line, 1);
        assert_eq!(cursor.column, 1);
    }

    #[test]
    fn backspace_across_line_join_clamps_cursor() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "ab\ncd");

        cursor.line = 1;
        cursor.column = 0;
        buffer.delete_previous_char(&mut cursor);

        assert_eq!(buffer.as_string(), "abcd");
        assert_eq!(cursor.line, 0);
        assert_eq!(cursor.column, 2);
    }

    #[test]
    fn delete_range_places_cursor_at_start() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "one two");

        buffer.delete_range(4, 7, &mut cursor);

        assert_eq!(buffer.as_string(), "one ");
        assert_eq!(cursor, Cursor { line: 0, column: 4 });
    }

    #[test]
    fn undo_restores_text_and_cursor() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "abc");

        assert!(buffer.undo(&mut cursor));

        assert_eq!(buffer.as_string(), "");
        assert_eq!(cursor, Cursor::default());
        assert!(!buffer.dirty);
    }

    #[test]
    fn undo_replace_range_is_atomic() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "- [ ] todo");
        buffer.undo_stack.clear();
        buffer.dirty = false;
        cursor = Cursor { line: 0, column: 0 };

        assert!(buffer.toggle_checkbox_at_line(0, &mut cursor));
        assert_eq!(buffer.as_string(), "[x] todo");
        assert_eq!(buffer.markdown_string(), "- [x] todo");

        assert!(buffer.undo(&mut cursor));
        assert_eq!(buffer.as_string(), "[ ] todo");
        assert_eq!(buffer.markdown_string(), "- [ ] todo");
        assert_eq!(cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn markdown_shortcut_becomes_facade_heading() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();

        buffer.insert_str(&mut cursor, "# Heading");

        assert_eq!(buffer.as_string(), "Heading");
        assert_eq!(buffer.markdown_string(), "# Heading");
        assert_eq!(cursor, Cursor { line: 0, column: 7 });
    }

    #[test]
    fn heading_shortcut_before_existing_text_maps_cursor_to_content_start() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "Heading");
        cursor = Cursor { line: 0, column: 0 };

        buffer.insert_char(&mut cursor, '#');
        buffer.insert_char(&mut cursor, ' ');

        assert_eq!(buffer.as_string(), "Heading");
        assert_eq!(buffer.markdown_string(), "# Heading");
        assert_eq!(cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn typed_checkbox_shortcut_after_bullet_facade_converts_to_checklist() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();

        for ch in "- [ ] todo".chars() {
            buffer.insert_char(&mut cursor, ch);
        }

        assert_eq!(buffer.as_string(), "[ ] todo");
        assert_eq!(buffer.markdown_string(), "- [ ] todo");
        assert_eq!(cursor, Cursor { line: 0, column: 8 });
    }

    #[test]
    fn inline_markdown_typing_remaps_cursor_to_plain_text() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();

        buffer.insert_str(&mut cursor, "a **bold** tail");

        assert_eq!(buffer.as_string(), "a bold tail");
        assert_eq!(buffer.markdown_string(), "a **bold** tail");
        assert_eq!(
            cursor,
            Cursor {
                line: 0,
                column: 11
            }
        );
    }

    #[test]
    fn surface_edit_changes_heading_level() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "## Heading");
        cursor = Cursor { line: 0, column: 0 };

        let display_column = buffer.replace_line_from_surface(0, "# Heading", 2, &mut cursor);

        assert_eq!(buffer.as_string(), "Heading");
        assert_eq!(buffer.markdown_string(), "# Heading");
        assert_eq!(display_column, 2);
        assert_eq!(cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn surface_edit_removing_heading_marker_converts_to_paragraph() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "# Heading");
        cursor = Cursor { line: 0, column: 0 };

        buffer.replace_line_from_surface(0, "Heading", 0, &mut cursor);

        assert_eq!(buffer.as_string(), "Heading");
        assert_eq!(buffer.markdown_string(), "Heading");
        assert_eq!(cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn surface_edit_updates_link_target() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "[README](README.md)");
        cursor = Cursor { line: 0, column: 2 };

        buffer.replace_line_from_surface(0, "[README](guide.md)", 17, &mut cursor);

        assert_eq!(buffer.as_string(), "README");
        assert_eq!(buffer.markdown_string(), "[README](guide.md)");
    }

    #[test]
    fn invalid_inline_surface_edit_degrades_to_plain_text() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "**bold**");
        cursor = Cursor { line: 0, column: 2 };

        buffer.replace_line_from_surface(0, "**bold*", 7, &mut cursor);

        assert_eq!(buffer.as_string(), "**bold*");
        assert_eq!(buffer.markdown_string(), "**bold*");
    }

    #[test]
    fn selected_range_serializes_back_to_markdown() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "# Heading");

        let selected =
            buffer.selected_markdown(Cursor { line: 0, column: 0 }, Cursor { line: 0, column: 7 });

        assert_eq!(selected.as_deref(), Some("# Heading"));
    }

    #[test]
    fn selected_full_blocks_do_not_insert_extra_blank_lines() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "# Heading\n- [ ] todo");

        let selected =
            buffer.selected_markdown(Cursor { line: 0, column: 0 }, Cursor { line: 1, column: 8 });

        assert_eq!(selected.as_deref(), Some("# Heading\n- [ ] todo"));
    }

    #[test]
    fn table_cell_navigation_moves_between_content_cells() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "| A | B |\n| --- | --- |\n| x | y |");
        cursor = Cursor { line: 0, column: 2 };

        assert!(buffer.move_table_cell(&mut cursor, 1));
        assert_eq!(cursor, Cursor { line: 0, column: 8 });

        assert!(buffer.move_table_cell(&mut cursor, 1));
        assert_eq!(cursor, Cursor { line: 2, column: 2 });

        assert!(buffer.move_table_cell(&mut cursor, -1));
        assert_eq!(cursor, Cursor { line: 0, column: 8 });
    }

    #[test]
    fn inserts_table_row_below_current_row() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "| A | B |\n| --- | --- |\n| x | y |");
        cursor = Cursor { line: 2, column: 2 };

        assert!(buffer.insert_table_row_at_cursor(&mut cursor, TableRowPlacement::Below));

        assert_eq!(
            buffer.markdown_string(),
            "| A   | B   |\n| --- | --- |\n| x   | y   |\n|     |     |"
        );
        assert_eq!(cursor.line, 3);
    }

    #[test]
    fn inserts_table_column_right_of_current_cell() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "| A | B |\n| --- | --- |\n| x | y |");
        cursor = Cursor { line: 2, column: 2 };

        assert!(buffer.insert_table_column_at_cursor(&mut cursor, TableColumnPlacement::Right));

        assert_eq!(
            buffer.markdown_string(),
            "| A   | Column 2 | B   |\n| --- | -------- | --- |\n| x   |          | y   |"
        );
        assert_eq!(cursor.line, 2);
    }

    #[test]
    fn enter_moves_to_next_table_row_or_adds_one() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "| A | B |\n| --- | --- |\n| x | y |");
        cursor = Cursor { line: 0, column: 2 };

        assert!(buffer.enter_table_cell(&mut cursor));
        assert_eq!(cursor.line, 2);

        assert!(buffer.enter_table_cell(&mut cursor));
        assert_eq!(cursor.line, 3);
        assert!(buffer.markdown_string().ends_with("\n|     |     |"));
    }

    #[test]
    fn delete_line_range_removes_selected_lines() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "a\nb\nc\n");

        buffer.delete_line_range(1, 2, &mut cursor);

        assert_eq!(buffer.as_string(), "a\n");
        assert_eq!(cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn delete_line_range_preserves_cursor_column_when_possible() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "short\nlonger line\nlast");
        cursor = Cursor { line: 0, column: 3 };

        buffer.delete_line_range(0, 0, &mut cursor);

        assert_eq!(buffer.as_string(), "longer line\nlast");
        assert_eq!(cursor, Cursor { line: 0, column: 3 });
    }

    #[test]
    fn trailing_file_newline_is_not_a_visible_ghost_line() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "a\n");

        assert_eq!(buffer.line_count(), 1);

        buffer.delete_line_range(0, 0, &mut cursor);

        assert_eq!(buffer.as_string(), "");
        assert_eq!(buffer.line_count(), 1);
        assert_eq!(cursor, Cursor { line: 0, column: 0 });
    }

    #[test]
    fn final_blank_line_can_be_deleted() {
        let mut buffer = DocumentBuffer::empty();
        let mut cursor = Cursor::default();
        buffer.insert_str(&mut cursor, "a\n\n");

        assert_eq!(buffer.line_count(), 2);

        buffer.delete_line_range(1, 1, &mut cursor);

        assert_eq!(buffer.as_string(), "a\n");
        assert_eq!(buffer.line_count(), 1);
        assert_eq!(cursor, Cursor { line: 0, column: 0 });
    }
}
