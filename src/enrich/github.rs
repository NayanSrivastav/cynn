use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrDetails {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub body_summary: Option<String>,
    pub reviewer_highlights: Vec<String>,
    pub linked_issues: Vec<String>,
    pub all_matched_prs: Vec<u64>,
}

#[derive(Deserialize)]
struct GhPrListJson {
    number: u64,
    title: String,
    url: String,
    body: Option<String>,
}

#[derive(Deserialize)]
struct GhAuthor {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhComment {
    body: Option<String>,
    author: Option<GhAuthor>,
}

#[derive(Deserialize)]
struct GhReview {
    body: Option<String>,
    author: Option<GhAuthor>,
    #[allow(dead_code)]
    state: Option<String>,
}

#[derive(Deserialize)]
struct GhIssueRef {
    number: Option<u64>,
}

#[derive(Deserialize)]
struct GhPrViewJson {
    number: u64,
    title: String,
    url: String,
    body: Option<String>,
    comments: Option<Vec<GhComment>>,
    reviews: Option<Vec<GhReview>>,
    #[serde(rename = "closingIssuesReferences")]
    closing_issues_references: Option<Vec<GhIssueRef>>,
}

#[derive(Deserialize)]
struct GhInlineComment {
    body: Option<String>,
    user: Option<GhAuthor>,
    path: Option<String>,
    line: Option<usize>,
}

/// Resolve Pull Request information, reviewer discussions, and linked issues for a commit SHA.
/// Executes with explicit repository working directory and optional remote owner/repo flag.
pub fn resolve_pr_for_commit(
    repo_root: &Path,
    remote_repo: Option<&str>,
    commit_sha: &str,
) -> Option<PrDetails> {
    // 1. Locate matching PRs for commit SHA (up to 5 to capture backports/merges)
    let mut list_cmd = Command::new("gh");
    list_cmd.args([
        "pr",
        "list",
        "--commit",
        commit_sha,
        "--json",
        "number,title,url,body",
        "--limit",
        "5",
    ]);
    list_cmd.current_dir(repo_root);
    if let Some(repo) = remote_repo {
        list_cmd.args(["-R", repo]);
    }

    let list_output = list_cmd.output().ok()?;
    if !list_output.status.success() {
        return None;
    }

    let prs: Vec<GhPrListJson> = serde_json::from_slice(&list_output.stdout).ok()?;
    if prs.is_empty() {
        return None;
    }

    let all_matched_prs: Vec<u64> = prs.iter().map(|p| p.number).collect();
    let matched_pr = &prs[0];

    // 2. Query detailed PR view with discussions, reviews, and closing issues
    let mut view_cmd = Command::new("gh");
    view_cmd.args([
        "pr",
        "view",
        &matched_pr.number.to_string(),
        "--json",
        "number,title,url,body,comments,reviews,closingIssuesReferences",
    ]);
    view_cmd.current_dir(repo_root);
    if let Some(repo) = remote_repo {
        view_cmd.args(["-R", repo]);
    }

    let mut inline_comments = Vec::new();
    if let Some(repo) = remote_repo {
        let endpoint = format!("repos/{repo}/pulls/{}/comments", matched_pr.number);
        let mut api_cmd = Command::new("gh");
        api_cmd.args(["api", &endpoint]);
        api_cmd.current_dir(repo_root);
        if let Ok(api_out) = api_cmd.output() {
            if api_out.status.success() {
                if let Ok(parsed) = serde_json::from_slice::<Vec<GhInlineComment>>(&api_out.stdout)
                {
                    inline_comments = parsed;
                }
            }
        }
    }

    if let Ok(view_out) = view_cmd.output() {
        if view_out.status.success() {
            if let Ok(details) = serde_json::from_slice::<GhPrViewJson>(&view_out.stdout) {
                return Some(build_pr_details(details, inline_comments, all_matched_prs));
            }
        }
    }

    // Fallback if detailed pr view fails
    let body_summary = matched_pr.body.as_ref().and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.lines().next().unwrap_or("").trim().to_string())
        }
    });

    let linked_issues = matched_pr
        .body
        .as_deref()
        .map(extract_issues_from_text)
        .unwrap_or_default();

    Some(PrDetails {
        number: matched_pr.number,
        title: matched_pr.title.clone(),
        url: matched_pr.url.clone(),
        body_summary,
        reviewer_highlights: Vec::new(),
        linked_issues,
        all_matched_prs,
    })
}

fn build_pr_details(
    view: GhPrViewJson,
    inline_comments: Vec<GhInlineComment>,
    all_matched_prs: Vec<u64>,
) -> PrDetails {
    let body_summary = view.body.as_ref().and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            let first_para = trimmed.split("\n\n").next().unwrap_or(trimmed).trim();
            Some(first_para.replace('\n', " "))
        }
    });

    let mut linked_issues = Vec::new();

    // 1. Add structured closing issue references
    if let Some(refs) = view.closing_issues_references {
        for issue_ref in refs {
            if let Some(num) = issue_ref.number {
                let tag = format!("#{num}");
                if !linked_issues.contains(&tag) {
                    linked_issues.push(tag);
                }
            }
        }
    }

    // 2. Parse text references from body and title
    if let Some(ref body) = view.body {
        for issue in extract_issues_from_text(body) {
            if !linked_issues.contains(&issue) {
                linked_issues.push(issue);
            }
        }
    }
    for issue in extract_issues_from_text(&view.title) {
        if !linked_issues.contains(&issue) {
            linked_issues.push(issue);
        }
    }

    let mut reviewer_highlights = Vec::new();

    // 3. Extract formal reviews (approvals, requested changes)
    if let Some(reviews) = view.reviews {
        for review in reviews {
            if let Some(body) = review.body {
                let clean = body.trim();
                if clean.len() > 10 {
                    let author = review
                        .author
                        .and_then(|a| a.login)
                        .unwrap_or_else(|| "reviewer".to_string());
                    let snippet: String = clean.chars().take(120).collect();
                    reviewer_highlights.push(format!("{author} (review): \"{snippet}\""));
                }
            }
        }
    }

    // 4. Extract conversation comments (do not skip if reviews exist!)
    if let Some(comments) = view.comments {
        for comment in comments {
            if let Some(body) = comment.body {
                let clean = body.trim();
                if clean.len() > 10 {
                    let author = comment
                        .author
                        .and_then(|a| a.login)
                        .unwrap_or_else(|| "collaborator".to_string());
                    let snippet: String = clean.chars().take(120).collect();
                    reviewer_highlights.push(format!("{author}: \"{snippet}\""));
                }
            }
        }
    }

    // 5. Extract inline code review thread comments
    for inline in inline_comments {
        if let Some(body) = inline.body {
            let clean = body.trim();
            if clean.len() > 10 {
                let author = inline
                    .user
                    .and_then(|u| u.login)
                    .unwrap_or_else(|| "reviewer".to_string());
                let loc = match (inline.path, inline.line) {
                    (Some(p), Some(l)) => format!(" on {p}:{l}"),
                    _ => String::new(),
                };
                let snippet: String = clean.chars().take(120).collect();
                reviewer_highlights.push(format!("{author}{loc}: \"{snippet}\""));
            }
        }
    }

    PrDetails {
        number: view.number,
        title: view.title,
        url: view.url,
        body_summary,
        reviewer_highlights,
        linked_issues,
        all_matched_prs,
    }
}

fn extract_issues_from_text(text: &str) -> Vec<String> {
    let mut issues = Vec::new();
    for word in text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '#');
        if let Some(num) = clean.strip_prefix('#') {
            if num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty() {
                let formatted = format!("#{num}");
                if !issues.contains(&formatted) {
                    issues.push(formatted);
                }
            }
        }
    }
    issues
}
