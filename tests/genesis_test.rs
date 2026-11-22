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
    assert!(dossier.genesis_commit.is_some());
    let genesis = dossier.genesis_commit.unwrap();
    assert_eq!(genesis.author_name, "Test Author");
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

    // Test 1: Query line 2 evolution (single-origin line)
    let target_line2 = Target {
        path: "engine.rs".into(),
        start_line: Some(2),
        end_line: Some(2),
    };
    let dossier2 = analyzer
        .analyze(&target_line2)
        .expect("Analysis failed for line 2");

    assert!(!dossier2.is_multi_origin);
    assert!(dossier2.genesis_commit.is_some());
    assert_eq!(
        dossier2.genesis_commit.as_ref().unwrap().subject,
        "feat: initial engine setup (#1)"
    );
    assert!(dossier2.evolution.len() >= 2);
    assert_eq!(dossier2.origins.len(), 1);
    assert!(dossier2.origins[0].history_resolved);

    // Test 2: Query multi-origin range lines 2-3
    let target_range = Target {
        path: "engine.rs".into(),
        start_line: Some(2),
        end_line: Some(3),
    };
    let dossier_range = analyzer
        .analyze(&target_range)
        .expect("Analysis failed for range");

    // Multi-origin block: genesis_commit is None (origins are authoritative)
    assert!(dossier_range.is_multi_origin);
    assert!(
        dossier_range.genesis_commit.is_none(),
        "Mixed-origin block must not assign an ambiguous single genesis commit"
    );
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

    assert!(dossier.genesis_commit.is_some());
    let genesis = dossier.genesis_commit.unwrap();
    assert_eq!(genesis.subject, "feat: add authenticate function (#10)");
    assert_eq!(dossier.pr_reference, Some("#10".to_string()));
}

#[test]
fn test_merge_commit_provenance() {
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

    run(&["init", "-b", "main"]);
    run(&["config", "user.name", "Nayan Srivastava"]);
    run(&["config", "user.email", "nayansrivastav@yahoo.in"]);

    // Initial commit on main
    let readme = repo_path.join("README.md");
    fs::write(&readme, "# Core System\n").unwrap();
    run(&["add", "README.md"]);
    run(&["commit", "-m", "chore: initial commit"]);

    // Feature branch introduces auth module
    run(&["checkout", "-b", "feature/auth"]);
    let auth = repo_path.join("auth.rs");
    fs::write(
        &auth,
        "pub fn verify_token(t: &str) -> bool {\n    !t.is_empty()\n}\n",
    )
    .unwrap();
    run(&["add", "auth.rs"]);
    run(&[
        "commit",
        "-m",
        "feat(auth): implement token verification (#42)",
    ]);

    // Back to main and merge with --no-ff
    run(&["checkout", "main"]);
    run(&[
        "merge",
        "--no-ff",
        "feature/auth",
        "-m",
        "Merge pull request #42 from feature/auth",
    ]);

    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let analyzer = GenesisAnalyzer::new(&repo);
    let target = Target {
        path: "auth.rs".into(),
        start_line: Some(1),
        end_line: Some(3),
    };
    let dossier = analyzer
        .analyze(&target)
        .expect("Failed to analyze auth.rs");

    assert!(dossier.genesis_commit.is_some());
    let genesis = dossier.genesis_commit.unwrap();
    assert_eq!(
        genesis.subject,
        "feat(auth): implement token verification (#42)"
    );
    assert_eq!(dossier.pr_reference, Some("#42".to_string()));
}

#[test]
fn test_squash_merge_provenance() {
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

    run(&["init", "-b", "main"]);
    run(&["config", "user.name", "Nayan Srivastava"]);
    run(&["config", "user.email", "nayansrivastav@yahoo.in"]);

    let readme = repo_path.join("README.md");
    fs::write(&readme, "# Base Project\n").unwrap();
    run(&["add", "README.md"]);
    run(&["commit", "-m", "chore: initial commit"]);

    // Feature branch
    run(&["checkout", "-b", "feature/cache"]);
    let cache = repo_path.join("cache.rs");
    fs::write(
        &cache,
        "pub fn get_cache(key: &str) -> Option<String> {\n    Some(key.to_string())\n}\n",
    )
    .unwrap();
    run(&["add", "cache.rs"]);
    run(&["commit", "-m", "feat(cache): implement redis client (#99)"]);

    // Squash merge into main
    run(&["checkout", "main"]);
    run(&["merge", "--squash", "feature/cache"]);
    run(&[
        "commit",
        "-m",
        "feat(cache): squash merge cache module (#99)\n\nSquashed commit of redis caching.",
    ]);

    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let analyzer = GenesisAnalyzer::new(&repo);
    let target = Target {
        path: "cache.rs".into(),
        start_line: Some(1),
        end_line: Some(3),
    };
    let dossier = analyzer
        .analyze(&target)
        .expect("Analysis failed for squash merge");

    assert!(dossier.genesis_commit.is_some());
    assert_eq!(dossier.pr_reference, Some("#99".to_string()));
    assert!(dossier.origins[0].history_resolved);
}

#[test]
fn test_rebased_commit_provenance() {
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

    run(&["init", "-b", "main"]);
    run(&["config", "user.name", "Nayan Srivastava"]);
    run(&["config", "user.email", "nayansrivastav@yahoo.in"]);

    let readme = repo_path.join("README.md");
    fs::write(&readme, "# Base\n").unwrap();
    run(&["add", "README.md"]);
    run(&["commit", "-m", "chore: base commit"]);

    // Branch off
    run(&["checkout", "-b", "feature/logger"]);
    let logger = repo_path.join("logger.rs");
    fs::write(
        &logger,
        "pub fn log_event(ev: &str) {\n    println!(\"{ev}\");\n}\n",
    )
    .unwrap();
    run(&["add", "logger.rs"]);
    run(&[
        "commit",
        "-m",
        "feat(logger): introduce structured logger (#55)",
    ]);

    // Advance main
    run(&["checkout", "main"]);
    fs::write(&readme, "# Base Project Advanced\n").unwrap();
    run(&["add", "README.md"]);
    run(&["commit", "-m", "docs: update readme on main"]);

    // Rebase feature/logger on main
    run(&["checkout", "feature/logger"]);
    run(&["rebase", "main"]);

    // Fast-forward merge into main
    run(&["checkout", "main"]);
    run(&["merge", "--ff-only", "feature/logger"]);

    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let analyzer = GenesisAnalyzer::new(&repo);
    let target = Target {
        path: "logger.rs".into(),
        start_line: Some(1),
        end_line: Some(3),
    };
    let dossier = analyzer
        .analyze(&target)
        .expect("Analysis failed for rebased commit");

    assert!(dossier.genesis_commit.is_some());
    assert_eq!(dossier.pr_reference, Some("#55".to_string()));
    assert_eq!(
        dossier.genesis_commit.unwrap().subject,
        "feat(logger): introduce structured logger (#55)"
    );
}

#[test]
fn test_multi_platform_gitlab_and_bitbucket_trailers() {
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

    // Configure a simulated GitLab origin
    run(&[
        "remote",
        "add",
        "origin",
        "git@gitlab.com:acme-org/analytics-service.git",
    ]);

    let file_path = repo_path.join("metrics.rs");
    fs::write(
        &file_path,
        "pub fn calculate_latency() -> u64 {\n    42\n}\n",
    )
    .unwrap();
    run(&["add", "metrics.rs"]);
    run(&[
        "commit",
        "-m",
        "Merge branch 'latency' into 'main'\n\nImplement p99 latency metric calculation\n\nSee merge request !145",
    ]);

    let repo = GitRepo::discover(repo_path).expect("Failed to discover repo");
    let remote_info = repo.get_remote_info().expect("Remote info should exist");
    assert_eq!(remote_info.platform, git_lineage::git::PlatformKind::GitLab);
    assert_eq!(remote_info.owner_repo, "acme-org/analytics-service");

    let analyzer = GenesisAnalyzer::new(&repo);
    let target = Target {
        path: "metrics.rs".into(),
        start_line: Some(1),
        end_line: Some(3),
    };
    let dossier = analyzer
        .analyze(&target)
        .expect("Analysis failed for GitLab repo");

    assert!(dossier.pr_details.is_some());
    let pr = dossier.pr_details.unwrap();
    assert_eq!(pr.number, 145);
    assert!(pr
        .url
        .contains("gitlab.com/acme-org/analytics-service/-/merge_requests/145"));
}
