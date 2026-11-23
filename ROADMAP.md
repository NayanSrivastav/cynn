# git-lineage: Roadmap & Feature Specifications

> **Tagline:** The Codebase Archaeology CLI and Agent Forensic Primitive.  
> Uncover why code was written, how it evolved, and the human decisions behind it—without the noise.

---

## 1. Vision & Architecture

### Core Value Proposition
`git-lineage` replaces the blind alleys of `git blame`. Instead of stopping at formatting commits, renames, or lint fixes, it performs deterministic graph traversal to surface:
1. **Genesis:** The true introducing commit, author, and date.
2. **Context:** The associated Pull Request, review discussion, and linked issue rationale.
3. **Evolution:** Key milestones where logic mutated across time, renames, and file moves.
4. **Agent Forensic Primitive:** A high-speed, structured context engine (`--json`) that AI coding agents (Claude Code, Cursor, Cline, Devin) query before refactoring to prevent regressions.

---

## 2. Technology Stack Selection: Why Rust?

| Criteria | **Rust** (Chosen) | **Go** | **TypeScript / Node** |
| :--- | :--- | :--- | :--- |
| **Git Object & DAG Traversal** | **Gold standard:** `git2` (libgit2) and fast CLI porcelain fallback | Good (`go-git`), but slower on huge repos | Poor (spawns heavy child processes) |
| **AST-Aware Tracking** | **`tree-sitter`** first-class zero-overhead bindings | Limited / manual CGO bindings | High overhead (`web-tree-sitter`) |
| **Binary Size & Startup** | Single ~8MB stripped binary, **<5ms startup** | Single ~15MB binary, ~15ms startup | Requires Node runtime (~60MB), ~150ms startup |
| **Agent / CLI Reliability** | Zero runtime dependencies, portable across macOS and Linux | Requires Go runtime, heavier memory footprint | Fragile dependency tree, slow cold-starts |

### Core Rust Crates:
* **CLI Parser:** `clap` (v4 with derive)
* **Git Operations:** Native Git porcelain with copy/move detection (`-w -C -C -M`) and libgit2
* **AST Parsing (v0.3+):** `tree-sitter` language grammars (Rust, Java, Python, Go, TypeScript/JS)
* **HTTP / Remote APIs:** `ureq` (lightweight, synchronous, minimal dependency footprint)
* **Styling & Presentation:** `owo-colors`, `comfy-table`
* **Serialization / Output:** `serde`, `serde_json`

---

## 3. Guiding Engineering Principles

1. **Deterministic & Truthful (Zero Hallucination):**  
   If a commit has no PR and the commit message is blank, state `[No rationale recorded]`. Never invent original intent or synthesize speculative explanations.
2. **Local-First & Offline Resilience:**  
   The core functionality works 100% offline using local git history. Remote GitHub/GitLab/Bitbucket/Jira integrations enrich provenance when credentials exist, but never block execution.
3. **Sub-second Performance:**  
   Target execution under **300ms** for line queries so it feels as fast as `git blame` or `ripgrep`.
4. **Agent-First Primitive:**  
   Every feature rendered in ANSI or Markdown is 1:1 serializable to machine-readable JSON for autonomous coding agents.

---

## 4. Milestone Progression

```
v0.1.0 "Genesis"       v0.2.x "Context"            v0.3.0 "Rationale & AST"       v0.4.0 "Standards & Audit"     v1.0.0 "Production"
 [Fast Local Core]  ─► [PR / Issue Extraction] ─►  [Jira/Linear + Tree-sitter] ─► [.git-blame-ignore + Audit] ─► [CI & Packaging]
```

---

### Milestone 1: v0.1.0 — "Genesis" (Completed)
* **Focus:** Replace `git blame` with true origin detection.
* **Delivered:**
  * Line and line-range targeting: `git lineage src/main.rs:42` and `:40-60`.
  * Automatic noise filtering: ignores whitespace (`-w`) and traces across renames (`-M`, `--follow`).
  * Provenance traversal: traverses commit history to isolate the true introducing commit SHA.
  * Terminal ANSI Dossier: Clean box-drawing output showing Genesis, Author, Date, and Commit Subject.
  * Markdown Export: `git lineage <target> --md` for documentation and pull request discussions.
  * 100% offline local git log execution.

---

### Milestone 2: v0.2.0 / v0.2.1 — "Context & Forensic Hardening" (Completed)
* **Focus:** Multi-platform remote enrichment, strict confidence tiers, and multi-origin attribution.
* **Delivered:**
  * **Multi-Platform Remote Context:** Deep PR/MR resolution for GitHub (`gh` CLI / REST API), GitLab (`glab` CLI / REST API), and Bitbucket.
  * **Multi-Origin Dissection:** Contiguous line grouping using whitespace- and copy-invariant blame (`-w -C -C -M`) preventing naive coordinate traversal into ancient pre-insertion commits.
  * **PR Rejection Audit Trail:** Explicitly logs disqualified candidate PRs in ANSI, Markdown, and JSON when issue keys mismatch (e.g. `!1097` COLL-7406 vs commit COLL-7431).
  * **Strict Confidence Scoring:** `High` (substantive PR/review rationale), `Medium` (substantive commit body), `Low` (subject-only), `None` (unrecorded).
  * **Companion Test Correlator:** Automatic discovery of related unit/integration test files without duplicate matches or self-matching.
  * **Function-Level Targeting & Diff:** Target functions by name (`file.rs:myFunction`, `-L :func:file`) and unified patch diff inspection (`--diff`).
  * **Agent Forensic Engine:** Structured, machine-readable `--json` output for AI coding agents.

---

### Milestone 3: v0.3.0 — "Rationale & AST" (Immediate Priority)
* **Focus:** Fix rationale emptiness via issue tracker connectors and eliminate line-drift attribution errors with Tree-sitter.
* **Deliverables:**
  * **Issue Tracker Connectors (Jira & Linear):**
    * In enterprise repos, commits routinely cite Jira/Linear keys (`COLL-7431`, `ENG-1024`) while PRs are squashed, rebased, or missing.
    * Direct REST integration for Jira Cloud/Server (`JIRA_BASE_URL`, `JIRA_API_TOKEN`) and Linear (`LINEAR_API_KEY`).
    * Surfaces issue summary, description, acceptance criteria, and resolution status directly into evidence, elevating unadorned commits to verifiable `High` or `Medium` confidence.
    * 100% verifiable recorded facts; strictly zero LLM hallucination.
  * **Tree-sitter AST & Symbol-Aware Tracking:**
    * Embed Tree-sitter grammars for major languages (Rust, Java, Python, Go, TypeScript/JavaScript).
    * Track enclosing syntax nodes (methods, classes, structs, match blocks) rather than fragile spatial line coordinates.
    * Eliminates the "lone line" ambiguity where small syntax lines (closing braces, `trim(e));`) yield variable attribution depending on requested line range.
    * Robust cross-file method relocation tracking beyond Git's heuristic copy detection.

---

### Milestone 4: v0.4.0 — "Standards & Archaeology Audit"
* **Focus:** Standard Git ecosystem compatibility and actionable codebase archaeology.
* **Deliverables:**
  * **Native `.git-blame-ignore-revs` Support:**
    * Automatically detect and respect standard `.git-blame-ignore-revs` files and repository `git config blame.ignoreRevsFile`.
    * Skips massive codebase reformatting, Prettier/Black passes, and license header migrations without inventing proprietary ignore configurations.
  * **Codebase Archaeology Audit (`git-lineage audit <dir>`):**
    * Replaces unbounded recursive dumps with actionable risk auditing.
    * Generates targeted reports surfacing:
      1. Critical functions/modules with zero recorded rationale (`[No rationale recorded]`).
      2. Dormant legacy code untouched for >24 months.
      3. High-churn hot spots with conflicting multi-origin lineage.

---

### Milestone 5: v1.0.0 — "Production & Distribution"
* **Focus:** Global packaging, CI governance, and IDE/agent distribution.
* **Deliverables:**
  * **GitHub Action (`git-lineage-action`):**
    * Automated PR review bot warning engineers when modifying legacy lines untouched for >2 years or touching code with critical architectural notes.
  * **Distribution & Packaging:**
    * Official Homebrew tap formula (`brew install nayansrivastav/tap/git-lineage`).
    * Crates.io publication (`cargo install git-lineage`).
    * Prebuilt signed release binaries for macOS (Apple Silicon & Intel) and Linux (x86_64, aarch64).
  * **Model Context Protocol (MCP) Server:**
    * Standalone MCP adapter exposing `git-lineage` directly to Cursor, Claude Code, and Cline.

---

## 5. Development Timeline & Status

| Milestone | Target Scope | Est. Duration | Status |
| :--- | :--- | :--- | :--- |
| **v0.1.0** | Core Genesis CLI (`--md`, local git traversal) | — | **Completed** |
| **v0.2.0 / v0.2.1** | Remote PR enrichment, multi-origin blame, audit trail, `--json`, `--diff` | — | **Completed** |
| **v0.3.0** | **Jira & Linear connectors**, **Tree-sitter AST tracking** | 1–2 Weeks | **Next (In Planning)** |
| **v0.4.0** | Native `.git-blame-ignore-revs`, `git-lineage audit <dir>` | 1–2 Weeks | Backlog |
| **v1.0.0** | GitHub Action PR bot, Homebrew Tap, Crates.io, MCP server | 1 Week | Backlog |
