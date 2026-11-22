use git_lineage::core::{Confidence, GenesisAnalyzer, Target};
use git_lineage::git::GitRepo;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_genesis_analyzer_on_fixture_repo() {
    let dir = tempdir().expect("Failed to create tempdir");
    let repo_path = dir.path();

    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .status()
            .expect("Failed to run git command");
        assert!(status.success(), "Git command failed: {args:?}");
    };

    run(&["init"]);
    run(&["config", "user.name", "Test Author"]);
    run(&["config", "user.email", "author@example.com"]);

    // 1. Commit 1: Genesis commit with tab indentation
    let file_path = repo_path.join("billing.ts");
    fs::write(
        &file_path,
        "export function refund(amount: number) {\n\tif (amount > 1000) return false;\n\treturn true;\n}\n",
    )
    .unwrap();

    run(&["add", "billing.ts"]);
    run(&[
        "commit",
        "-m",
        "feat: introduce refund limits (#42)\n\nPrevents refunds exceeding $1000 without manual approval.",
    ]);

    // 2. Commit 2: Formatting commit (tabs converted to 4 spaces with trailing spaces)
    fs::write(
        &file_path,
        "export function refund(amount: number) {\n    if (amount > 1000) return false;   \n    return true;\n}\n",
    )
    .unwrap();

    run(&["add", "billing.ts"]);
    run(&["commit", "-m", "chore: prettier formatting"]);

    // Verify that raw git blame WITHOUT -w attributes line 2 to Commit 2
    let raw_blame = Command::new("git")
        .args(["blame", "--line-porcelain", "-L2,2", "billing.ts"])
        .current_dir(repo_path)
        .output()
        .expect("git blame failed");
    let raw_out = String::from_utf8_lossy(&raw_blame.stdout);
    assert!(
        raw_out.contains("summary chore: prettier formatting"),
        "Raw blame without -w should point to formatting commit"
    );

    // 3. Run git-lineage analysis on line 2
    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let target = Target {
        path: "billing.ts".into(),
        start_line: Some(2),
        end_line: Some(2),
    };

    let analyzer = GenesisAnalyzer::new(&repo);
    let dossier = analyzer.analyze(&target).expect("Analysis failed");

    // 4. Verify that git-lineage correctly bypassed the prettier commit and identified Genesis (#42)
    assert_eq!(dossier.genesis_commit.author_name, "Test Author");
    assert_eq!(dossier.pr_reference, Some("#42".to_string()));
    assert!(dossier
        .intent_summary
        .contains("Prevents refunds exceeding $1000"));
    assert_eq!(dossier.confidence, Confidence::Medium);
    assert!(!dossier.evidence.is_empty());
}

#[test]
fn test_evolution_milestones_and_multi_origin_block() {
    let dir = tempdir().expect("Failed to create tempdir");
    let repo_path = dir.path();

    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .status()
            .expect("Failed to run git command");
        assert!(status.success(), "Git command failed: {args:?}");
    };

    run(&["init"]);
    run(&["config", "user.name", "Nayan Srivastava"]);
    run(&["config", "user.email", "nayansrivastav@yahoo.in"]);

    let file_path = repo_path.join("engine.rs");

    // Commit 1: Initial creation of lines 1-3
    fs::write(&file_path, "fn start() {\n    let count = 0;\n}\n").unwrap();
    run(&["add", "engine.rs"]);
    run(&["commit", "-m", "feat: initial engine setup (#1)"]);

    // Commit 2: Line 4 added (multi-origin introduction)
    fs::write(
        &file_path,
        "fn start() {\n    let count = 0;\n    let max = 100;\n}\n",
    )
    .unwrap();
    run(&["add", "engine.rs"]);
    run(&["commit", "-m", "feat: add max limit (#2)"]);

    // Commit 3: Line 2 modified (evolution milestone)
    fs::write(
        &file_path,
        "fn start() {\n    let count = 10;\n    let max = 100;\n}\n",
    )
    .unwrap();
    run(&["add", "engine.rs"]);
    run(&["commit", "-m", "fix: adjust initial count (#3)"]);

    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let analyzer = GenesisAnalyzer::new(&repo);

    // Test 1: Query line 2 evolution
    let target_line2 = Target {
        path: "engine.rs".into(),
        start_line: Some(2),
        end_line: Some(2),
    };
    let dossier2 = analyzer
        .analyze(&target_line2)
        .expect("Analysis failed for line 2");

    // The true genesis of line 2 was Commit 1, despite being edited in Commit 3
    assert_eq!(
        dossier2.genesis_commit.subject,
        "feat: initial engine setup (#1)"
    );
    assert!(dossier2.evolution.len() >= 2);
    assert_eq!(dossier2.origins.len(), 1);
    assert!(dossier2.origins[0].is_verified);

    // Test 2: Query multi-origin range lines 2-3
    let target_range = Target {
        path: "engine.rs".into(),
        start_line: Some(2),
        end_line: Some(3),
    };
    let dossier_range = analyzer
        .analyze(&target_range)
        .expect("Analysis failed for range");

    assert_eq!(dossier_range.origins.len(), 2);
    assert_eq!(dossier_range.origins[0].line_start, 2);
    assert_eq!(dossier_range.origins[0].line_end, 2);
    assert_eq!(dossier_range.origins[1].line_start, 3);
    assert_eq!(dossier_range.origins[1].line_end, 3);
    assert!(dossier_range.origins[1].summary.contains("add max limit"));
}

#[test]
fn test_code_moved_down() {
    let dir = tempdir().expect("Failed to create tempdir");
    let repo_path = dir.path();

    let run = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .status()
            .expect("Failed to run git command");
        assert!(status.success(), "Git command failed: {args:?}");
    };

    run(&["init"]);
    run(&["config", "user.name", "Nayan Srivastava"]);
    run(&["config", "user.email", "nayansrivastav@yahoo.in"]);

    let file_path = repo_path.join("service.rs");

    // Commit 1: Function at the top of file (lines 1-3)
    fs::write(&file_path, "pub fn authenticate() -> bool {\n    true\n}\n").unwrap();
    run(&["add", "service.rs"]);
    run(&["commit", "-m", "feat: add authenticate function (#10)"]);

    // Commit 2: Add 20 lines of header imports and comments above it
    let mut header = String::new();
    for i in 1..=20 {
        header.push_str(&format!("// Header comment line {i}\n"));
    }
    header.push_str("pub fn authenticate() -> bool {\n    true\n}\n");

    fs::write(&file_path, header).unwrap();
    run(&["add", "service.rs"]);
    run(&[
        "commit",
        "-m",
        "chore: add license and architecture comments",
    ]);

    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let analyzer = GenesisAnalyzer::new(&repo);

    // Query authenticate function at its new position (lines 21-23)
    let target = Target {
        path: "service.rs".into(),
        start_line: Some(21),
        end_line: Some(23),
    };
    let dossier = analyzer
        .analyze(&target)
        .expect("Analysis failed for moved code");

    // Tracing must correctly track through the position offset to Commit 1
    assert_eq!(
        dossier.genesis_commit.subject,
        "feat: add authenticate function (#10)"
    );
    assert_eq!(dossier.pr_reference, Some("#10".to_string()));
}
