use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrDetails {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub body_summary: Option<String>,
    pub reviewer_highlights: Vec<String>,
    pub linked_issues: Vec<String>,
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
struct GhPrViewJson {
    number: u64,
    title: String,
    url: String,
    body: Option<String>,
    comments: Option<Vec<GhComment>>,
    reviews: Option<Vec<GhReview>>,
}

/// Resolve Pull Request information, reviewer discussions, and linked issues for a commit SHA
pub fn resolve_pr_for_commit(commit_sha: &str) -> Option<PrDetails> {
    // 1. Locate matching PR number for commit SHA
    let list_output = Command::new("gh")
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

    if !list_output.status.success() {
        return None;
    }

    let prs: Vec<GhPrListJson> = serde_json::from_slice(&list_output.stdout).ok()?;
    let matched_pr = prs.into_iter().next()?;

    // 2. Query detailed PR view with discussions and reviews
    let view_output = Command::new("gh")
        .args([
            "pr",
            "view",
            &matched_pr.number.to_string(),
            "--json",
            "number,title,url,body,comments,reviews",
        ])
        .output()
        .ok();

    if let Some(output) = view_output {
        if output.status.success() {
            if let Ok(details) = serde_json::from_slice::<GhPrViewJson>(&output.stdout) {
                return Some(build_pr_details(details));
            }
        }
    }

    // Fallback if pr view fails
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
        title: matched_pr.title,
        url: matched_pr.url,
        body_summary,
        reviewer_highlights: Vec::new(),
        linked_issues,
    })
}

fn build_pr_details(view: GhPrViewJson) -> PrDetails {
    let body_summary = view.body.as_ref().and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            // Find first substantive paragraph
            let first_para = trimmed.split("\n\n").next().unwrap_or(trimmed).trim();
            Some(first_para.replace('\n', " "))
        }
    });

    let linked_issues = view
        .body
        .as_deref()
        .map(extract_issues_from_text)
        .unwrap_or_default();

    let mut reviewer_highlights = Vec::new();

    // Extract substantive review comments
    if let Some(reviews) = view.reviews {
        for review in reviews {
            if let Some(body) = review.body {
                let clean = body.trim();
                if !clean.is_empty() && clean.len() > 10 {
                    let author = review
                        .author
                        .and_then(|a| a.login)
                        .unwrap_or_else(|| "reviewer".to_string());
                    let snippet: String = clean.chars().take(120).collect();
                    reviewer_highlights.push(format!("{author}: \"{snippet}\""));
                }
            }
        }
    }

    // Extract discussion comments if reviews were empty
    if reviewer_highlights.is_empty() {
        if let Some(comments) = view.comments {
            for comment in comments {
                if let Some(body) = comment.body {
                    let clean = body.trim();
                    if !clean.is_empty() && clean.len() > 10 {
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
    }

    PrDetails {
        number: view.number,
        title: view.title,
        url: view.url,
        body_summary,
        reviewer_highlights,
        linked_issues,
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
