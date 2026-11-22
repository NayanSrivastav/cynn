# git-lineage

<div align="center">

<h3>The Codebase Archaeology CLI and Agent Forensic Primitive</h3>

<p>
  <strong>Uncover why code was written, how it evolved, and the human decisions behind it—without the noise.</strong>
</p>

[![CI](https://github.com/NayanSrivastav/git-lineage/actions/workflows/ci.yml/badge.svg)](https://github.com/NayanSrivastav/git-lineage/actions)
[![Rust](https://img.shields.io/badge/rust-1.78%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

[Quickstart](#quickstart) •
[Why Not `git blame`?](#why-not-git-blame) •
[Features](#features) •
[CLI Reference](#cli-reference) •
[Architecture](#architecture) •
[AI / Agent Integration](#ai--agent-integration) •
[Contributing](#contributing)

</div>

---

## The Problem: The Anatomy of a Failed `git blame`

It is 4:00 PM on a Tuesday. You are reviewing unfamiliar legacy code in your payments service:

```typescript
// src/billing/stripe_webhook.ts
if (event.id.startsWith("evt_test_") && payload.attempt > 1) {
  return 200; // Why is this check here? Is it safe to delete?
}
```

You run standard `git blame`:

```text
a3f89b1 Bob 2024-02-10 "refactor: run prettier across billing"
```

**Dead end.** Bob left the company two years ago, and the commit was an automated formatting pass that modified 800 files.

To find the truth, you must now:
1. Run `git log -S "evt_test_" -p` to hunt through raw diffs.
2. Step through 12 merge commits and file renames.
3. Copy commit hashes into GitHub search to find the original Pull Request.
4. Read through closed issues and reviewer comments to understand the edge case.

**Result: 40 minutes of cognitive drain for a 3-line check.**

---

## The Solution: `git lineage`

Run `git-lineage` directly on the file and line:

```bash
git lineage src/billing/stripe_webhook.ts:88
```

Within **sub-second time**, `git-lineage` filters out cosmetic noise, traces code through renames, correlates the true commit with GitHub PR discussions, and outputs an archaeological dossier:

```text
╭─ Lineage: src/billing/stripe_webhook.ts:88 ───────────────────────────────────────╮
│ Code: if (event.id.startsWith("evt_test_") && payload.attempt > 1) { return 200; } │
╰───────────────────────────────────────────────────────────────────────────────────╯

📌 Genesis (Introducing Commit)
   Commit:    e819b2c (2022-04-12, 2 years ago)
   Author:    Sarah Chen <sarah@acme.dev>
   PR:        #412: "Handle duplicate sandbox webhook retries from Stripe API v2"
   Issue:     #398 (Closed)
   Intent:    "Stripe test-mode webhooks retry up to 5x even on 200 in sandbox when
               fixtures take >2s. This prevents infinite retry storms in CI."

📜 Evolution Timeline
   2022-04-12  PR #412   Born: Created in `src/legacy/webhooks.ts` by Sarah Chen
   2023-08-19  PR #904   Moved: Refactored & renamed to `src/billing/stripe_webhook.ts`
   2024-02-10  PR #1420  Cosmetic: Prettier formatting update (ignored)

🧪 Blast Radius & Evidence
   Related Test:   tests/billing/stripe_webhook.test.ts:44 ("handles sandbox retry storm")
   Confidence:     High (Direct PR rationale and linked issue found)
   Current Expert: Sarah Chen (5 commits in this file), Marcus Vance (last touched logic)
```

---

## Why Not Existing Tools?

| Capability | `git blame` | `git log -S` | GitLens (VS Code) | `git-lineage` |
| :--- | :---: | :---: | :---: | :---: |
| **Bypasses formatting / lint noise** | ❌ | ❌ | Partial (`-w`) | **Yes (Automatic heuristic)** |
| **Follows code across file renames** | ❌ | Complex (`--follow`) | ❌ | **Yes (Automatic DAG tracking)** |
| **Surfaces PR rationale & discussions** | ❌ | ❌ | Yes (GUI only) | **Yes (Terminal & Markdown)** |
| **Locates companion test cases** | ❌ | ❌ | ❌ | **Yes (Heuristic test matcher)** |
| **Interactive Terminal TUI** | ❌ | ❌ | ❌ | **Yes (`ratatui` dual-pane)** |
| **Recursive directory eras** | ❌ | ❌ | ❌ | **Yes (`-r` / `--recursive`)** |
| **Machine-readable for AI agents** | ❌ | ❌ | ❌ | **Yes (`--json` MCP-ready)** |
| **Runtime footprint** | Built-in | Built-in | Heavy (Node/Electron) | **Single static binary (<8MB)** |

---

## Quickstart

### Installation

#### macOS (Homebrew)
```bash
brew install git-lineage
```

#### Linux & macOS (Automated Script)
```bash
curl -fsSL https://raw.githubusercontent.com/NayanSrivastav/git-lineage/main/install.sh | sh
```

#### From Source (Rust toolchain)
```bash
cargo install git-lineage
```

### Basic Commands

```bash
# Trace a specific line
git lineage src/billing/stripe_webhook.ts:88

# Trace a block of lines
git lineage src/billing/stripe_webhook.ts:80-120

# Launch interactive terminal TUI
git lineage src/billing/stripe_webhook.ts:88 -i

# Recursive directory architecture analysis
git lineage src/billing/ -r

# Export clean Markdown for PR reviews or Slack
git lineage src/billing/stripe_webhook.ts:88 --md

# Machine-readable JSON output
git lineage src/billing/stripe_webhook.ts:88 --json
```

---

## Features

### 1. True Genesis Detection
Standard `git blame` stops at the most recent commit touching a line. `git-lineage` traverses parent commits, ignoring whitespace deltas (`-w`), mass code formatting commits (via `.git-blame-ignore-revs` or automatic heuristic detection), and file renames (`-M`, `--follow`) until it locates the **true introducing commit**.

### 2. Context & Rationale Extraction
Code commits tell you *what* changed; Pull Requests tell you *why*. `git-lineage` maps commit SHAs to GitHub/GitLab Pull Requests and issues:
* Extracts the PR title and description summary.
* Locates linked issue tickets (e.g., `#398`, `ENG-1240`).
* Highlights key discussion points from code reviewers.

### 3. Interactive Time-Travel TUI (`-i`)
When launched in an interactive terminal, `git-lineage` provides a full-screen, dual-pane TUI powered by `ratatui`:
* **Left Pane:** Chronological milestone timeline (Born → Moved → Refactored → Current).
* **Right Pane:** Side-by-side diff view and PR commentary for the selected milestone.
* **Keybindings:** Vim navigation (`j`/`k`, `g`/`G`), `Enter` to expand diff, `o` to open PR in browser, `y` to copy Markdown dossier to clipboard, `q` to quit.

### 4. Recursive Directory Archaeology (`-r`)
Run `git lineage <dir> -r` to analyze an entire module or directory:
* Reconstructs the **Architectural Eras** of a subsystem (e.g., *Foundation Era* vs. *V2 Migration Era*).
* Surfaces **Dormant / High-Risk Zones** (files untouched for >18 months whose original authors have left).
* Maps active maintainers and module ownership.

### 5. Truthful Confidence Scoring
`git-lineage` operates on strict engineering integrity:
* `[Explicit]`: Verified PR rationale and linked issue tickets found.
* `[Inferred]`: Rationale extracted from commit message body and diff patterns.
* `[None]`: Commit message contains no explanation; no PR exists. **It never hallucinates reasons.**

---

## Architecture & Data Flow

```
┌────────────────────────────────────────────────────────┐
│                   CLI / Input Target                   │
│         (e.g., src/billing/webhook.ts:88)              │
└──────────────────────────┬─────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│                   Noise Filter Stage                   │
│   • Whitespace normalization (-w)                      │
│   • Ignore-revs registry (.git-blame-ignore-revs)     │
│   • Rename & copy tracking (-M, -C, --follow)          │
└──────────────────────────┬─────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│                Git DAG Traversal Engine                │
│   • Fast in-memory graph walking via `gix`/`git2`      │
│   • AST syntax node tracking (Tree-sitter)            │
│   • Pinpoint Genesis SHA & Evolution Milestones        │
└──────────────────────────┬─────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│               Context Enrichment Engine                │
│   • Local commit trailer parsing (Co-authored-by, etc) │
│   • GitHub / GitLab PR & Issue resolution (gh CLI/API) │
│   • Companion test matching (tests/**/*.test.*)        │
└──────────────────────────┬─────────────────────────────┘
                           │
                           ▼
┌────────────────────────────────────────────────────────┐
│                   Output Formatter                     │
│   ├── Terminal ANSI View (Default)                     │
│   ├── Interactive TUI View (-i)                        │
│   ├── Markdown Dossier (--md)                          │
│   └── Machine JSON Schema (--json / MCP)               │
└────────────────────────────────────────────────────────┘
```

---

## AI / Agent Integration (MCP-Ready)

In the era of AI-assisted engineering, agents like **Claude Code**, **Cursor**, and **Devin** routinely break legacy edge cases because they only see the static workspace snapshot.

`git-lineage` provides a high-density, structured context primitive that agents query before refactoring:

```json
{
  "target": "src/billing/stripe_webhook.ts:88",
  "genesis": {
    "commit": "e819b2c8",
    "author": "Sarah Chen <sarah@acme.dev>",
    "date": "2022-04-12T14:22:10Z",
    "pr": {
      "number": 412,
      "title": "Handle duplicate sandbox webhook retries from Stripe API v2",
      "summary": "Stripe test-mode webhooks retry up to 5x even on 200 in sandbox..."
    }
  },
  "confidence": "explicit",
  "related_tests": ["tests/billing/stripe_webhook.test.ts:44"]
}
```

This prevents AI regressions by providing the exact business constraints that created the code.

---

## Privacy & Security

* **100% Local by Default:** All Git operations occur on your local machine. No code is transmitted to any cloud server.
* **Optional Remote Access:** GitHub/GitLab integration uses your local authenticated `gh`/`glab` CLI credentials or personal access token.
* **No Telemetry:** Zero tracking, analytics, or background phone-home requests.

---

## Contributing

We welcome contributions from engineers who appreciate high-craft developer tooling!
Please read our [Contributing Guide](CONTRIBUTING.md) and [Development Guidelines](DEVELOPMENT.md) before submitting pull requests.

---

## License

This project is licensed under the [MIT License](LICENSE).
