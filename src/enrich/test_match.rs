use std::path::Path;

/// Locate companion test files associated with a given source file path
pub fn find_related_tests(repo_root: &Path, file_path: &Path) -> Vec<String> {
    let mut matches = Vec::new();

    let Some(stem) = file_path.file_stem().and_then(|s| s.to_str()) else {
        return matches;
    };

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    // Common test naming patterns:
    // foo_test.rs, foo.test.ts, test_foo.py, foo_spec.rb, tests/foo_test.rs
    let candidate_names = [
        format!("{stem}_test.{ext}"),
        format!("{stem}.test.{ext}"),
        format!("{stem}.spec.{ext}"),
        format!("test_{stem}.{ext}"),
    ];

    // Check sibling directory or adjacent tests/ folder
    let search_dirs = [
        repo_root.join("tests"),
        repo_root.join("test"),
        file_path
            .parent()
            .map(|p| repo_root.join(p))
            .unwrap_or_else(|| repo_root.to_path_buf()),
    ];

    for dir in &search_dirs {
        if !dir.is_dir() {
            continue;
        }

        for candidate in &candidate_names {
            let direct_path = dir.join(candidate);
            if direct_path.exists() {
                if let Ok(rel) = direct_path.strip_prefix(repo_root) {
                    let rel_str = rel.display().to_string();
                    if !matches.contains(&rel_str) {
                        matches.push(rel_str);
                    }
                }
            }
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_find_related_test_file() {
        let dir = tempdir().unwrap();
        let tests_dir = dir.path().join("tests");
        fs::create_dir_all(&tests_dir).unwrap();

        let test_file = tests_dir.join("payment_test.rs");
        fs::write(&test_file, "// test").unwrap();

        let related = find_related_tests(dir.path(), Path::new("src/payment.rs"));
        assert_eq!(related, vec!["tests/payment_test.rs"]);
    }
}
