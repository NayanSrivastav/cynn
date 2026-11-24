# GEMINI.md: Project Development Guidelines & Standards

This document establishes the official engineering guidelines, architectural invariants, and code standards for `cynn`. All AI pair programmers and human contributors must adhere to these standards from Day 1.

---

## 1. Project Invariants

1. **Production-Grade Engineering:**  
   Every line of code must adhere to production-ready standards. No temporary hacks, no hardcoded magic paths, and no suppressed compiler warnings.
2. **Deterministic & Truthful (Zero Hallucination):**  
   The core value of `cynn` is trust. If a commit has no PR, or a commit message is blank, explicitly output `[No rationale recorded]`. Never invent, guess, or extrapolate developer intent.
3. **Local-First & Offline Resilience:**  
   The core engine must run 100% offline against local Git repositories. Remote calls (GitHub/GitLab) must be strictly optional enhancements that fail gracefully without crashing.
4. **Sub-second Performance Budget:**  
   Line-level queries must return in **<300ms**. Avoid unnecessary Git packfile parsing, redundant object lookups, or unbuffered I/O.
5. **No Telemetry or Phone-Home:**  
   `cynn` operates on private corporate codebases. It must never transmit user code, commit hashes, or telemetry over the network.

---

## 2. Rust Code Standards & Idioms

### Error Handling
* **Library Code (`src/`):** Use explicit, strongly typed error enums powered by **`thiserror`**.
* **CLI Boundary (`src/bin/` / `main.rs`):** Use **`anyhow::Result`** for ergonomic, user-friendly top-level error reporting.
* **No `unwrap()` or `expect()` in Production Code:**
  * Disallowed in all non-test code. Use `?` operator, `map_err`, or explicit match arms.
  * Use `.expect("invariant: explanation")` strictly in scenarios where a prior check guarantees safety.

### Lints & Formatting
* Code must compile cleanly with zero warnings under:
  ```bash
  cargo clippy --all-targets --all-features -- -D warnings
  cargo fmt --all -- --check
  ```
* Enforce Clippy pedantic rules where practical. Prefer explicit types over ambiguous type inference on public API surfaces.

### Memory & Performance
* Prefer borrowing (`&str`, `&Path`, `&[u8]`) over cloning (`String`, `PathBuf`) where possible.
* Use streaming iterators over collecting entire commit histories into large heap allocations.

---

## 3. Architecture & Module Boundaries

The codebase is organized into clean, decoupled layers:

```
src/
├── lib.rs              # Public library interface
├── main.rs             # CLI entrypoint (clap parser, signal handling)
├── core/
│   ├── mod.rs
│   ├── genesis.rs      # Provenance traversal & true origin detection
│   ├── noise.rs        # Whitespace, formatting, and rename noise filters
│   └── ast.rs          # Syntax-aware symbol tracking (Tree-sitter)
├── git/
│   ├── mod.rs
│   ├── repo.rs         # Safe wrappers around Git operations (gix / git2)
│   └── diff.rs         # Delta computation and line mapping
├── enrich/
│   ├── mod.rs
│   ├── provider.rs     # Trait for remote enrichment (GitHub, GitLab)
│   ├── github.rs       # GitHub CLI (`gh`) and REST API client
│   └── test_match.rs   # Heuristic companion test locator
├── tui/
│   ├── mod.rs
│   ├── app.rs          # Ratatui application state machine
│   └── ui.rs           # Dual-pane rendering and key event handlers
└── output/
    ├── mod.rs
    ├── ansi.rs         # Box-drawing terminal formatter
    ├── markdown.rs     # GitHub-flavored Markdown generator
    └── json.rs         # Structured JSON schema for AI agents / MCP
```

---

## 4. Git & Commit Guidelines

We strictly follow the **Conventional Commits** specification:

* `feat: ...` — New feature or capability.
* `fix: ...` — Bug fix.
* `docs: ...` — Documentation updates.
* `perf: ...` — Performance optimization.
* `refactor: ...` — Code refactoring without behavior changes.
* `test: ...` — Adding or updating test suites.
* `chore: ...` — Build tools, dependencies, CI configuration.

**Branch Naming:**
* `feat/<short-description>`
* `fix/<short-description>`
* `docs/<short-description>`

---

## 5. Testing Requirements

No feature or bug fix will be merged without automated tests:

1. **Unit Tests:** Placed adjacent to source files (`mod tests { ... }`) for isolated business logic, noise filtering, and formatters.
2. **Integration Tests (`tests/`):**
   * Use a helper to initialize a temporary Git repository in a tempdir (`tempfile` crate).
   * Create realistic commit histories (whitespace commits, renames, squashes).
   * Run the `cynn` engine against the fixture repo to verify origin detection.
3. **Deterministic Fixtures:** Never rely on network calls in automated tests; mock all GitHub/GitLab API responses.
