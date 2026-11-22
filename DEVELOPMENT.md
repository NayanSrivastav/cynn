# Development Guide: git-lineage

This guide provides technical instructions for developing, testing, and profiling `git-lineage`.

---

## 1. Local Environment Setup

Ensure you have the Rust toolchain installed:

```bash
# Install Rust via rustup if not already present
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add formatting and linting components
rustup component add rustfmt clippy
```

---

## 2. Running Locally as a Git Subcommand

Git automatically detects any executable in your `$PATH` named `git-<name>` and allows invoking it as `git <name>`.

To test `git lineage` against local repos while developing:

```bash
# In the git-lineage directory:
cargo build

# Create a symlink to cargo's debug binary in your local bin or path
ln -sf $(pwd)/target/debug/git-lineage /usr/local/bin/git-lineage
# or in ~/.cargo/bin
ln -sf $(pwd)/target/debug/git-lineage ~/.cargo/bin/git-lineage

# Now run it anywhere:
git lineage src/main.rs:1
```

---

## 3. Testing Strategy

### Unit Tests
Unit tests are co-located in `src/` modules:
```bash
cargo test --lib
```

### Integration Tests with Temporary Git Fixtures
Integration tests in `tests/` spin up programmatic Git repositories using the `tempfile` crate. They simulate real-world developer scenarios:
* Repositories with multi-line squash merges.
* Repositories with automated Prettier/Black formatting passes.
* Repositories where files were renamed or split into multiple modules.

Run integration tests:
```bash
cargo test --test integration
```

---

## 4. Benchmarking & Performance Budgets

`git-lineage` has a strict performance budget: **<300ms** for line queries in repositories with up to 100,000 commits.

We use [`hyperfine`](https://github.com/sharkdp/hyperfine) for benchmarking:

```bash
# Compare git-lineage with raw git blame
hyperfine --warmup 3 \
  'git lineage src/main.rs:50' \
  'git blame -L 50,50 src/main.rs'
```

---

## 5. Pre-Commit Checklist

Always run this one-liner before submitting code:

```bash
cargo fmt --all && cargo clippy --all-targets --all-features -- -D warnings && cargo test
```
