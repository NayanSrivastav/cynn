use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrDetails {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub body_summary: Option<String>,
}

#[derive(Deserialize)]
struct GhPrJson {
    number: u64,
    title: String,
    url: String,
    body: Option<String>,
}

/// Resolve Pull Request information for a given commit SHA via the local `gh` CLI
pub fn resolve_pr_for_commit(commit_sha: &str) -> Option<PrDetails> {
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--commit",
            commit_sha,
            "--json",
            "number,title,url,body",
            "--limit",
            "1",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let prs: Vec<GhPrJson> = serde_json::from_slice(&output.stdout).ok()?;
    let pr = prs.into_iter().next()?;

    let body_summary = pr.body.and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            let first_line = trimmed.lines().next().unwrap_or("").trim().to_string();
            Some(first_line)
        }
    });

    Some(PrDetails {
        number: pr.number,
        title: pr.title,
        url: pr.url,
        body_summary,
    })
}
