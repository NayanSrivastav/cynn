use git_lineage::core::GenesisAnalyzer;
use git_lineage::core::Target;
use git_lineage::git::GitRepo;
use std::fs;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn test_genesis_analyzer_on_fixture_repo() {
    let dir = tempdir().expect("Failed to create tempdir");
    let repo_path = dir.path();

    // 1. Initialize temporary git repository
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

    // 2. Commit 1: Genesis commit
    let file_path = repo_path.join("billing.ts");
    fs::write(
        &file_path,
        "export function refund(amount: number) {\n  if (amount > 1000) return false;\n  return true;\n}\n",
    )
    .unwrap();

    run(&["add", "billing.ts"]);
    run(&[
        "commit",
        "-m",
        "feat: introduce refund limits (#42)\n\nPrevents refunds exceeding $1000 without manual approval.",
    ]);

    // 3. Commit 2: Whitespace formatting commit (noise)
    fs::write(
        &file_path,
        "export function refund(amount: number) {\n    if (amount > 1000) return false;\n    return true;\n}\n",
    )
    .unwrap();

    run(&["add", "billing.ts"]);
    run(&["commit", "-m", "chore: prettier formatting"]);

    // 4. Run git-lineage analysis on line 2
    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let target = Target {
        path: "billing.ts".into(),
        start_line: Some(2),
        end_line: Some(2),
    };

    let analyzer = GenesisAnalyzer::new(&repo);
    let dossier = analyzer.analyze(&target).expect("Analysis failed");

    // 5. Verify that it correctly skipped the prettier commit and found the genesis commit (#42)
    assert_eq!(dossier.genesis_commit.author_name, "Test Author");
    assert_eq!(dossier.pr_reference, Some("#42".to_string()));
    assert!(dossier
        .intent_summary
        .contains("Prevents refunds exceeding $1000"));
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
    // Evolution should track both commits (Commit 3 mutation and Commit 1 genesis)
    assert!(dossier2.evolution.len() >= 2);
    assert_eq!(dossier2.origins.len(), 1);

    // Test 2: Query multi-origin range lines 2-3
    let target_range = Target {
        path: "engine.rs".into(),
        start_line: Some(2),
        end_line: Some(3),
    };
    let dossier_range = analyzer
        .analyze(&target_range)
        .expect("Analysis failed for range");
    // Line 2 and line 3 originated from different commits:
    // Line 2 was created in Commit 1 (and modified in Commit 3)
    // Line 3 ("let max = 100;") was created in Commit 2
    assert_eq!(dossier_range.origins.len(), 2);
    assert_eq!(dossier_range.origins[0].line_start, 2);
    assert_eq!(dossier_range.origins[0].line_end, 2);
    assert_eq!(dossier_range.origins[1].line_start, 3);
    assert_eq!(dossier_range.origins[1].line_end, 3);
    assert!(dossier_range.origins[1].summary.contains("add max limit"));
}
