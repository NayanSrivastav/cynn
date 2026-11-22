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

    // Deduplicate search directories to prevent duplicate discovery
    let mut search_dirs = Vec::new();
    let default_tests = repo_root.join("tests");
    if default_tests.is_dir() {
        search_dirs.push(default_tests);
    }
    let default_test = repo_root.join("test");
    if default_test.is_dir() && !search_dirs.contains(&default_test) {
        search_dirs.push(default_test);
    }
    if let Some(parent) = file_path.parent() {
        let parent_dir = repo_root.join(parent);
        if parent_dir.is_dir() && !search_dirs.contains(&parent_dir) {
            search_dirs.push(parent_dir);
        }
    }

    let file_path_str = file_path.to_str().unwrap_or("");

    for dir in &search_dirs {
        for candidate in &candidate_names {
            let direct_path = dir.join(candidate);
            if direct_path.exists() {
                if let Ok(rel) = direct_path.strip_prefix(repo_root) {
                    let rel_str = rel.display().to_string();
                    // Never match the target file itself, and avoid duplicate matches
                    if rel_str != file_path_str && !matches.contains(&rel_str) {
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

    #[test]
    fn test_find_related_test_no_duplicates_and_no_self_match() {
        let dir = tempdir().unwrap();
        let tests_dir = dir.path().join("tests");
        fs::create_dir_all(&tests_dir).unwrap();

        let test_file = tests_dir.join("engine_test.rs");
        fs::write(&test_file, "// test").unwrap();

        // Target is the test file itself: must not return itself
        let self_related = find_related_tests(dir.path(), Path::new("tests/engine_test.rs"));
        assert!(!self_related.contains(&"tests/engine_test.rs".to_string()));

        // Target in src: must return exactly one match without duplicates
        let related = find_related_tests(dir.path(), Path::new("src/engine.rs"));
        assert_eq!(related, vec!["tests/engine_test.rs"]);
    }
}
