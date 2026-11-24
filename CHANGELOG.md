# Changelog

All notable changes to `git-lineage` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Planned
- **v0.3.0**: Model Context Protocol (MCP) server stdio adapter (`--mcp`).
- **v0.3.0**: Jira REST API (`JIRA_BASE_URL` + `JIRA_API_TOKEN`) and Linear API (`LINEAR_API_KEY`) connectors.
- **v0.3.0**: Tree-sitter AST & symbol-aware tracking for methods, classes, and syntax blocks.
- **v0.4.0**: Native `.git-blame-ignore-revs` and `git config blame.ignoreRevsFile` support.
- **v0.4.0**: Codebase archaeology audit command (`git-lineage audit <dir>`).

---

## [0.2.3] - 2026-11-24

### Fixed
- **Platform-Linked MR Association**: Prevent false disqualifications when a commit carries no Jira ticket key (e.g., "temp" or WIP commits) but the platform-linked Merge Request provides the ticket key. The MR is now accurately associated.
- **Bundle & Hotfix MR Handling**: Platform-linked MRs with mismatched ticket keys (e.g. commit COLL-7391 inside bundle MR !622 titled COLL-7357) are no longer rejected with `pr_details: None`. They are accepted as associated bundle MRs with review discussions preserved and an explicit audit warning recorded.
- **Cross-File Copy Attribution**: Added `-C -C -M` copy and rename detection flags to `trace_line_evolution` and prevented line history walkback from overwriting cross-file copy-detected genesis commits discovered by deep blame.
- **Network Latency & Milestone Fallback**: Eliminated redundant sequential HTTP network requests by adding in-memory session caching for remote commit PR lookups and bounding milestone fallback queries to at most 3 commits.

---

## [0.2.2] - 2026-11-23

### Added
- **Rejected PR Audit Trail**: Explicitly records and outputs candidate Pull Requests / Merge Requests disqualified due to Jira/issue key conflicts (e.g. GitLab MR `!1097` COLL-7406 rejected against commit COLL-7431) across ANSI, Markdown, and JSON.
- **Function-Level Targeting**: Support targeting functions by name using `path/to/file.ext:function_name` or `-L :func:file` syntax.
- **Unified Diff Patch Inspection**: Added `-p` / `--diff` flag to output the unified patch from the introducing genesis commit.
- **Dual Blame Attribution**: Each origin slice now records both its authoritative genesis commit and its latest HEAD blame commit (`blame_commits` and `blame_sha`) for side-by-side comparison.

### Fixed
- **Multi-Origin Line Attribution Bug**: Fixed an issue where naive coordinate tracing via `git log -L` walked backward past line insertion boundaries into ancient predecessor commits (e.g., falsely attributing lines 1810–1816 of `MessageSyncDelegateImpl.java` to `4730eb6db`). Line origins are now accurately dissected via whitespace- and copy-invariant blame (`-w -C -C -M`).
- **Multi-Origin Genesis Semantics**: Multi-origin ranges explicitly assign `genesis_commit: null`, attribute distinct introducing commits per slice, and prune milestones older than the earliest introducing commit of the block.
- **Historical Milestones Preservation**: Fixed timeline shadowing in single-origin blocks to preserve all intermediate evolution revisions and assign exactly one genesis marker.
- **Issue Key Delimiter Parsing**: Expanded token parsing delimiters to include `/`, `:`, `(`, `)`, `[`, `]`, `,` to accurately isolate issue keys from branch names like `feature/COLL-7431`.
- **Confidence Scoring Enforcement**: Enforced strict confidence evaluation ensuring subject-only branch-name commits evaluate to `Low` confidence.

---

## [0.2.1] - 2026-11-23

### Added
- Automated GitHub Actions multi-platform binary release pipeline for macOS (Apple Silicon `aarch64` and Intel `x86_64`) and Linux (`x86_64`).
- Universal one-line installer script (`install.sh`) with automated SHA-256 checksum verification and architecture detection.
- Homebrew formula (`Formula/git-lineage.rb`) for native package management.

---

## [0.2.0] - 2026-11-22

### Added
- Multi-platform remote context resolution supporting GitHub (`gh` CLI / REST API), GitLab (`glab` CLI / REST API), and Bitbucket.
- Pull request discussion and reviewer comment classification (`is_technical`).
- Companion test correlation engine discovering related test files (`*.test.*`, `*_spec.*`) without duplicate or self-matching entries.
- Machine-readable `--json` schema output designed for autonomous coding agents (Claude Code, Cursor, Cline).
- Documentation portal and landing page at `https://nayansrivastav.github.io/git-lineage/`.

---

## [0.1.0] - 2026-11-22

### Added
- Initial release: local-first provenance engine replacing `git blame` blind alleys.
- Deterministic Git DAG traversal isolating true introducing genesis commits across renames (`-M`, `--follow`) and whitespace churn (`-w`).
- Terminal ANSI dossier box rendering and GitHub-flavored Markdown export (`--md`).
