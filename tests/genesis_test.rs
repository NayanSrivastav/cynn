use git_lineage::core::Target;
use git_lineage::core::GenesisAnalyzer;
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
        assert!(status.success(), "Git command failed: {:?}", args);
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
