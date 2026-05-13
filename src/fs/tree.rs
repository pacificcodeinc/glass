use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub display_name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone)]
pub struct FileTree {
    pub entries: Vec<TreeEntry>,
    pub selected: usize,
}

impl FileTree {
    pub fn load(root: &Path) -> Result<Self> {
        let mut entries = Vec::new();
        let mut walker = WalkBuilder::new(root);
        walker
            .hidden(true)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .filter_entry(|entry| should_visit(entry.path()));

        for result in walker.build() {
            let entry = result?;
            let path = entry.path();
            if path == root {
                continue;
            }

            let file_type = entry.file_type();
            let is_dir = file_type.is_some_and(|kind| kind.is_dir());
            let is_markdown = path.extension().is_some_and(|ext| ext == "md");

            if !is_dir && !is_markdown {
                continue;
            }

            let display_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();

            entries.push(TreeEntry {
                path: path.to_path_buf(),
                display_name,
                is_dir,
            });
        }

        entries.sort_by(|left, right| {
            left.path
                .parent()
                .cmp(&right.path.parent())
                .then_with(|| right.is_dir.cmp(&left.is_dir))
                .then_with(|| left.display_name.cmp(&right.display_name))
        });

        let selected = entries
            .iter()
            .position(|entry| !entry.is_dir)
            .unwrap_or_default();

        Ok(Self { entries, selected })
    }

    pub fn selected_file(&self) -> Option<&Path> {
        self.entries
            .get(self.selected)
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path.as_path())
    }
}

fn should_visit(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };

    !matches!(
        name,
        ".git" | "target" | "node_modules" | ".direnv" | ".DS_Store"
    )
}
