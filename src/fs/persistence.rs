use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

pub fn load_utf8(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

pub fn save_atomic(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory: {}", parent.display()))?;
    }

    let temp_path = temp_path_for(path);
    {
        let mut file = fs::File::create(&temp_path)
            .with_context(|| format!("failed to create {}", temp_path.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("failed to write {}", temp_path.display()))?;
        file.sync_all()
            .with_context(|| format!("failed to sync {}", temp_path.display()))?;
    }

    fs::rename(&temp_path, path)
        .with_context(|| format!("failed to replace {}", path.display()))?;
    Ok(())
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("glass")
        .to_string();
    name.push_str(".tmp");
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_utf8_file() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("glassnotes-test-{}", std::process::id()));
        fs::create_dir_all(&dir)?;
        let path = dir.join("note.md");

        save_atomic(&path, "# Test\n")?;
        assert_eq!(load_utf8(&path)?, "# Test\n");

        fs::remove_file(path)?;
        fs::remove_dir(dir)?;
        Ok(())
    }

    #[test]
    fn save_creates_missing_parent_directories() -> Result<()> {
        let dir = std::env::temp_dir().join(format!(
            "glassnotes-nested-save-test-{}",
            std::process::id()
        ));
        let path = dir.join("nested").join("note.md");

        save_atomic(&path, "# Test\n")?;
        assert_eq!(load_utf8(&path)?, "# Test\n");

        fs::remove_dir_all(dir)?;
        Ok(())
    }
}
