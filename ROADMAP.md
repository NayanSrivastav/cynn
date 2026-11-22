# git-lineage: Roadmap & Feature Specifications

> **Tagline:** The Codebase Archaeology CLI and Agent Forensic Primitive.  
> Uncover why code was written, how it evolved, and the human decisions behind it—without the noise.

---

## 1. Vision & Architecture

### Core Value Proposition
`git-lineage` replaces the blind alleys of `git blame`. Instead of stopping at formatting commits, renames, or lint fixes, it performs deterministic graph traversal to surface:
1. **Genesis:** The true introducing commit, author, and date.
2. **Context:** The associated Pull Request, review discussion, and linked issue rationale.
3. **Evolution:** Key milestones where the logic mutated across time and file moves.
4. **Agent Primitive:** A high-speed, structured context engine (`--json`) that AI coding agents (Claude Code, Cursor, Devin) query before refactoring to prevent regressions.

---

## 2. Technology Stack Selection: Why Rust?

| Criteria | **Rust** (Recommended) | **Go** | **TypeScript / Node** |
| :--- | :--- | :--- | :--- |
| **Git Object & DAG Traversal** | **Gold standard:** `git2` (libgit2) or `gix` (pure Rust Git) | Good (`go-git`), but slower on huge repos | Poor (spawns heavy child processes) |
| **TUI Ecosystem** | **`ratatui` + `crossterm`** (dominant, 60fps) | `bubbletea` (clean, but heavier memory) | `ink` (resource heavy for terminal) |
| **AST-Aware Tracking** | **`tree-sitter`** first-class bindings | Limited / manual CGO bindings | Good (`web-tree-sitter`) |
| **Binary Size & Startup** | Single ~8MB stripped binary, **<5ms startup** | Single ~15MB binary, ~15ms startup | Requires Node runtime (~60MB), ~150ms startup |
| **Community Reputation** | The hallmark of beloved CLI tools (`ripgrep`, `bat`, `delta`, `eza`, `starship`) | Strong (`gh`, `lazygit`, `fzf`) | Weak for low-level systems CLI |

### Core Rust Crates:
* **CLI Parser:** `clap` (v4 with derive)
* **Git Engine:** `gix` (pure Rust, safe, fast) or `git2` (libgit2 bindings)
* **Interactive TUI:** `ratatui` + `crossterm`
* **Styling & Presentation:** `owo-colors`, `comfy-table`, `syntect`
* **AST Parsing (v0.4+):** `tree-sitter`
* **Serialization / Output:** `serde`, `serde_json`

---

## 3. Guiding Engineering Principles

1. **Deterministic & Truthful (Zero Hallucination):**  
   If a commit has no PR and the commit message is blank, state `[No rationale recorded]`. Never invent original intent.
2. **Local-First:**  
   The core functionality must work 100% offline using local git history. Remote GitHub/GitLab integration is an enhancement layer.
3. **Sub-second Performance:**  
   Target execution under **300ms** for line queries so it feels as fast as `git blame` or `ripgrep`.
4. **Agent-Compatible:**  
   Every feature rendered in the terminal must be 1:1 serializable to machine-readable JSON for LLM tool-calling.

---

## 4. 5-Release Milestone Payload

```
v0.1.0 "Genesis"       v0.2.0 "Context"       v0.3.0 "Time-Travel"      v0.4.0 "Deep Lineage"     v1.0.0 "Production"
 [Fast Local Core]  ─► [PR / Issue Links] ─►   [Interactive TUI]   ─►   [Recursive & AST]   ─►  [CI & Distribution]
```

### Milestone 1: v0.1.0 — "Genesis" (The Fast Local Core)
* **Focus:** Replace `git blame` with true origin detection.
* **Deliverables:**
  * Line and line-range targeting: `git lineage src/main.rs:42` and `:40-60`.
  * Automatic noise filtering: ignores whitespace (`-w`) and traces across renames (`-M`, `--follow`).
  * Provenance traversal: traverses commit history to isolate the true introducing commit SHA.
  * Terminal ANSI Dossier: Clean box-drawing output showing Genesis, Author, Date, and Commit Message.
  * Markdown Export: `git lineage <target> --md` for copy-pasting into discussions.
  * Zero remote calls; runs 100% locally from git logs.

### Milestone 2: v0.2.0 — "Context" (PR & Ecosystem Enrichment)
* **Focus:** Bridge git commits with human team rationale.
* **Deliverables:**
  * Remote resolution: Queries local `gh` / `glab` CLI or GitHub API to map commit SHAs to Pull Requests.
  * Context extraction: Extracts PR title, PR body excerpt, and reviewer discussion highlights for that diff.
  * Evidence rating: Displays confidence markers `[Explicit: PR + Issue]`, `[Inferred: Commit Body]`, or `[None]`.
  * Test Correlator: Heuristic lookup of companion test files (`*.test.*`, `*_spec.*`).
  * Machine Output: `--json` schema designed for Claude Code, Cursor, and MCP integration.

### Milestone 3: v0.3.0 — "Time-Travel" (The Interactive TUI)
* **Focus:** Make code exploration visual and tactile.
* **Deliverables:**
  * Full-screen interactive terminal interface using `ratatui`.
  * Timeline Scrubber: Scroll through chronological evolutions using Vim keys (`j`/`k`, `g`/`G`).
  * Dual-Pane View: Left pane shows chronological milestones; right pane displays commit diffs and PR commentary.
  * Quick-actions:
    * `Enter` to expand diff details.
    * `o` to open the PR in the default browser.
    * `y` to yank the Markdown dossier to the clipboard.
    * `/` to search and filter commits.

### Milestone 4: v0.4.0 — "Deep Lineage" (Recursive & AST Archaeology)
* **Focus:** Module-level architecture and dependency tracking.
* **Deliverables:**
  * **Directory Recursion (`-r` / `--recursive`):**
    * Summarizes the architectural eras of an entire folder (`git lineage src/billing/ -r`).
    * Flags dormant / high-risk code untouched for >18 months.
  * **Dependency / Call-Graph Recursion (`--deep`):**
    * Traces why upstream caller functions were modified in previous PRs.
  * **AST / Symbol Tracking (`-s <symbol>`):**
    * Language-aware tracking using Tree-sitter so function refactors across files are tracked reliably.

### Milestone 5: v1.0.0 — "Production" (Enterprise & Ecosystem Standard)
* **Focus:** Global packaging, CI safety, and issue trackers.
* **Deliverables:**
  * `.gitlineageignore`: Ignore massive refactors, bulk formatting, or licensing rebrand commits.
  * Issue Tracker Connectors: Automatic resolution of Jira, Linear, and Asana keys (`ENG-123`, `PAY-99`).
  * GitHub Action (`git-lineage-action`): Automated PR bot warning developers when modifying legacy lines untouched for >2 years.
  * Packaging & Distribution: Homebrew tap, Cargo registry, npm wrapper binary, and prebuilt binaries for macOS (ARM/x86) and Linux.

---

## 5. Development Timeline & Estimates

| Milestone | Target Scope | Est. Duration | Status |
| :--- | :--- | :--- | :--- |
| **v0.1.0** | Core Genesis CLI (`--md`, local git traversal) | 1–2 Weeks | In Planning |
| **v0.2.0** | GitHub/GitLab PR Context & `--json` | 1 Week | Backlog |
| **v0.3.0** | Ratatui Interactive TUI | 1–2 Weeks | Backlog |
| **v0.4.0** | Recursive Directory & AST Tracing | 2 Weeks | Backlog |
| **v1.0.0** | Ignore lists, CI Action, Homebrew release | 1 Week | Backlog |
