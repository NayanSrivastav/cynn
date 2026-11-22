# git-lineage

> **The Codebase Archaeology CLI and Agent Forensic Primitive.**  
> Uncover why code was written, how it evolved, and the human decisions behind it—without the noise.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

---

## The Problem

You are looking at line 88 of an unfamiliar file:

```typescript
// src/billing/stripe_webhook.ts
if (event.id.startsWith("evt_test_") && payload.attempt > 1) {
  return 200; // Why is this here? Is it safe to delete?
}
```

* **Standard `git blame`:**  
  `a3f89b1 Bob 2024-02-10 "refactor: prettier formatting across billing"`  
  *(Dead end. Bob left 2 years ago, and the commit was just whitespace).*
* **The Manual Archaeological Slog:**  
  Running `git log -S`, reading 14 merge commits, cross-referencing GitHub PRs, checking Jira tickets. **40 minutes wasted.**

---

## The Solution

Run `git-lineage`:

```bash
git lineage src/billing/stripe_webhook.ts:88
```

In **sub-second time**, `git-lineage` traverses the Git DAG, ignores whitespace and rename noise, resolves the parent Pull Request, and surfaces the human intent:

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
   2024-02-10  PR #1420  Cosmetic: Prettier indentation update (ignored)

🧪 Blast Radius & Evidence
   Related Test:   tests/billing/stripe_webhook.test.ts:44 ("handles sandbox retry storm")
   Confidence:     High (Direct PR rationale and linked issue found)
   Current Expert: Sarah Chen (5 commits in this file), Marcus Vance (last touched logic)
```

---

## Key Capabilities

1. **True Origin Detection:** Automatically skips formatting commits, lint updates, and tracks code through renames (`--follow`).
2. **Context Enrichment:** Resolves commit hashes to Pull Requests, issue discussions, and review comments.
3. **Interactive TUI Mode (`-i`):** A dual-pane terminal interface to scrub through historical milestones, view diffs, and inspect PR discussions with Vim keys.
4. **Recursive Lineage (`-r`):** Discover the architectural eras and dormant high-risk zones across an entire directory.
5. **Agent-Ready (`--json`):** Formatted context payload for AI agents (Claude Code, Cursor, Devin) to prevent regressions during refactors.

---

## Usage

```bash
# Target a specific line
git lineage src/billing/stripe_webhook.ts:88

# Target a block of lines
git lineage src/billing/stripe_webhook.ts:80-120

# Launch interactive terminal explorer
git lineage src/billing/stripe_webhook.ts:88 -i

# Recursive directory overview
git lineage src/billing/ -r

# Export clean Markdown for PR descriptions or Slack
git lineage src/billing/stripe_webhook.ts:88 --md

# Machine-readable output for AI agents and scripts
git lineage src/billing/stripe_webhook.ts:88 --json
```

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the 5-release breakdown from `v0.1.0` (Fast Local Core) to `v1.0.0` (Production & Ecosystem).

---

## License

MIT
