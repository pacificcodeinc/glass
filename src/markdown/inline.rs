#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    Url,
    Markdown,
    Wiki,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineLink {
    pub kind: LinkKind,
    pub target: String,
    pub label: Option<String>,
    pub source_start: usize,
    pub source_end: usize,
    pub label_start: Option<usize>,
    pub label_end: Option<usize>,
    pub target_start: usize,
    pub target_end: usize,
}

impl InlineLink {
    pub fn contains_column(&self, column: usize) -> bool {
        column >= self.source_start && column < self.source_end
    }
}

pub fn links(source: &str) -> Vec<InlineLink> {
    let chars: Vec<char> = source.chars().collect();
    let mut links = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '`' {
            index = match find_next(&chars, index + 1, '`') {
                Some(end) => end + 1,
                None => index + 1,
            };
            continue;
        }

        if chars[index] == '[' {
            if let Some(link) = wiki_link_at(&chars, index) {
                index = link.source_end;
                links.push(link);
                continue;
            }

            if let Some(link) = markdown_link_at(&chars, index) {
                index = link.source_end;
                links.push(link);
                continue;
            }
        }

        if chars[index] == '<' {
            if let Some(link) = autolink_at(&chars, index) {
                index = link.source_end;
                links.push(link);
                continue;
            }
        }

        if let Some(link) = bare_url_at(&chars, index) {
            index = link.source_end;
            links.push(link);
            continue;
        }

        index += 1;
    }

    links
}

pub fn link_at_column(source: &str, column: usize) -> Option<InlineLink> {
    links(source)
        .into_iter()
        .find(|link| link.contains_column(column))
}

fn markdown_link_at(chars: &[char], start: usize) -> Option<InlineLink> {
    let close_bracket = find_next(chars, start + 1, ']')?;
    if chars.get(close_bracket + 1) != Some(&'(') {
        return None;
    }

    let target_start = close_bracket + 2;
    let close_paren = find_next(chars, target_start, ')')?;
    if target_start == close_paren {
        return None;
    }

    Some(InlineLink {
        kind: LinkKind::Markdown,
        target: collect(chars, target_start, close_paren),
        label: Some(collect(chars, start + 1, close_bracket)),
        source_start: start,
        source_end: close_paren + 1,
        label_start: Some(start + 1),
        label_end: Some(close_bracket),
        target_start,
        target_end: close_paren,
    })
}

fn wiki_link_at(chars: &[char], start: usize) -> Option<InlineLink> {
    if chars.get(start + 1) != Some(&'[') {
        return None;
    }

    let close = find_token(chars, start + 2, "]]")?;
    if start + 2 == close {
        return None;
    }

    let target = collect(chars, start + 2, close);
    Some(InlineLink {
        kind: LinkKind::Wiki,
        target: target.clone(),
        label: Some(target),
        source_start: start,
        source_end: close + 2,
        label_start: Some(start + 2),
        label_end: Some(close),
        target_start: start + 2,
        target_end: close,
    })
}

fn autolink_at(chars: &[char], start: usize) -> Option<InlineLink> {
    let close = find_next(chars, start + 1, '>')?;
    let target_start = start + 1;
    if !is_url_start(chars, target_start) {
        return None;
    }

    Some(InlineLink {
        kind: LinkKind::Url,
        target: collect(chars, target_start, close),
        label: None,
        source_start: start,
        source_end: close + 1,
        label_start: None,
        label_end: None,
        target_start,
        target_end: close,
    })
}

fn bare_url_at(chars: &[char], start: usize) -> Option<InlineLink> {
    if !is_url_boundary(chars, start) || !is_url_start(chars, start) {
        return None;
    }

    let mut end = start;
    while end < chars.len() && !chars[end].is_whitespace() {
        if matches!(chars[end], '<' | '>' | '"' | '\'') {
            break;
        }
        end += 1;
    }

    end = trim_url_end(chars, start, end);
    if start == end {
        return None;
    }

    Some(InlineLink {
        kind: LinkKind::Url,
        target: collect(chars, start, end),
        label: None,
        source_start: start,
        source_end: end,
        label_start: None,
        label_end: None,
        target_start: start,
        target_end: end,
    })
}

fn is_url_boundary(chars: &[char], start: usize) -> bool {
    if start == 0 {
        return true;
    }

    chars
        .get(start.saturating_sub(1))
        .is_some_and(|ch| ch.is_whitespace() || matches!(ch, '(' | '[' | '{'))
}

fn is_url_start(chars: &[char], start: usize) -> bool {
    starts_with(chars, start, "https://")
        || starts_with(chars, start, "http://")
        || starts_with(chars, start, "www.")
}

fn trim_url_end(chars: &[char], start: usize, mut end: usize) -> usize {
    while end > start {
        let last = chars[end - 1];
        if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | ']') {
            end -= 1;
            continue;
        }

        if last == ')' && has_unmatched_close_paren(&chars[start..end]) {
            end -= 1;
            continue;
        }

        break;
    }

    end
}

fn has_unmatched_close_paren(chars: &[char]) -> bool {
    let opens = chars.iter().filter(|ch| **ch == '(').count();
    let closes = chars.iter().filter(|ch| **ch == ')').count();
    closes > opens
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

fn collect(chars: &[char], start: usize, end: usize) -> String {
    chars[start..end].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_markdown_links() {
        let found = links("see [Glass](https://github.com/pacificcodeinc/glass)");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, LinkKind::Markdown);
        assert_eq!(found[0].label.as_deref(), Some("Glass"));
        assert_eq!(found[0].target, "https://github.com/pacificcodeinc/glass");
    }

    #[test]
    fn finds_bare_urls_and_trims_sentence_punctuation() {
        let found = links("open https://example.com/path.");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, LinkKind::Url);
        assert_eq!(found[0].target, "https://example.com/path");
    }

    #[test]
    fn keeps_balanced_url_parentheses() {
        let found = links("open https://example.com/wiki/Foo_(bar)");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target, "https://example.com/wiki/Foo_(bar)");
    }

    #[test]
    fn trims_closing_sentence_parenthesis() {
        let found = links("(https://example.com/path)");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].target, "https://example.com/path");
    }

    #[test]
    fn ignores_urls_inside_inline_code() {
        let found = links("not `https://example.com` here");

        assert!(found.is_empty());
    }

    #[test]
    fn finds_wiki_links() {
        let found = links("see [[Daily Note]]");

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, LinkKind::Wiki);
        assert_eq!(found[0].target, "Daily Note");
    }

    #[test]
    fn locates_link_under_cursor() {
        let source = "see [Glass](glass.md)";
        let found = link_at_column(source, 6).unwrap();

        assert_eq!(found.target, "glass.md");
    }
}
