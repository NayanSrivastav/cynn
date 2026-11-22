use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Review comment or discussion highlight with clean author attribution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewHighlight {
    pub author: String,
    pub body: String,
    pub review_state: Option<String>,
    pub url: Option<String>,
    pub is_technical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrDetails {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub body_summary: Option<String>,
    pub reviewer_highlights: Vec<ReviewHighlight>,
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
    url: Option<String>,
}

#[derive(Deserialize)]
struct GhReview {
    body: Option<String>,
    author: Option<GhAuthor>,
    state: Option<String>,
    url: Option<String>,
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
    #[serde(rename = "html_url")]
    html_url: Option<String>,
}

/// Distinguish substantive technical rationale from generic conversational approvals
pub fn is_technical_rationale(body: &str) -> bool {
    let trimmed = body.trim();
    if trimmed.len() < 25 {
        return false;
    }
    let lower = trimmed.to_lowercase();
    let trivial_patterns = [
        "looks good",
        "lgtm",
        "thanks",
        "thank you",
        "approved",
        "+1",
        "nice catch",
        "done",
        "fixed",
        "merged",
        "good to me",
        "ship it",
        "great work",
        "kudos",
    ];
    !trivial_patterns
        .iter()
        .any(|p| lower == *p || (lower.starts_with(p) && trimmed.len() < 35))
}

/// Discover GitHub API authentication token from environment or gh CLI credentials
pub fn resolve_github_token() -> Option<String> {
    if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        let t = token.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    let out = Command::new("gh").args(["auth", "token"]).output().ok()?;
    if out.status.success() {
        let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    None
}

/// Direct HTTPS query to api.github.com for commit PRs (zero CLI binary required)
#[allow(clippy::too_many_lines)]
pub fn fetch_github_pr_api(token: &str, owner_repo: &str, commit_sha: &str) -> Option<PrDetails> {
    let list_url = format!("https://api.github.com/repos/{owner_repo}/commits/{commit_sha}/pulls");
    let mut res = ureq::get(&list_url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "git-lineage")
        .header("Accept", "application/vnd.github+json")
        .call()
        .ok()?;

    let prs_val: serde_json::Value = res.body_mut().read_json().ok()?;
    let arr = prs_val.as_array()?;
    if arr.is_empty() {
        return None;
    }

    let all_matched_prs: Vec<u64> = arr.iter().filter_map(|p| p["number"].as_u64()).collect();
    let first = &arr[0];
    let number = first["number"].as_u64()?;
    let title = first["title"].as_str().unwrap_or("").to_string();
    let pr_url = first["html_url"].as_str().unwrap_or("").to_string();
    let body = first["body"].as_str().map(String::from);

    let body_summary = body.as_ref().and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            let first_para = trimmed.split("\n\n").next().unwrap_or(trimmed).trim();
            Some(first_para.replace('\n', " "))
        }
    });

    let mut linked_issues = Vec::new();
    if let Some(ref b) = body {
        for issue in extract_issues_from_text(b) {
            if !linked_issues.contains(&issue) {
                linked_issues.push(issue);
            }
        }
    }
    for issue in extract_issues_from_text(&title) {
        if !linked_issues.contains(&issue) {
            linked_issues.push(issue);
        }
    }

    let mut reviewer_highlights = Vec::new();

    // 1. Reviews
    let reviews_url = format!("https://api.github.com/repos/{owner_repo}/pulls/{number}/reviews");
    if let Ok(mut rev_res) = ureq::get(&reviews_url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "git-lineage")
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        if let Ok(revs) = rev_res.body_mut().read_json::<serde_json::Value>() {
            if let Some(rev_arr) = revs.as_array() {
                for r in rev_arr {
                    let r_body = r["body"].as_str().unwrap_or("").trim();
                    if r_body.len() > 10 {
                        let author = r["user"]["login"]
                            .as_str()
                            .unwrap_or("reviewer")
                            .to_string();
                        let state = r["state"].as_str().map(String::from);
                        let url = r["html_url"].as_str().map(String::from);
                        reviewer_highlights.push(ReviewHighlight {
                            author,
                            body: r_body.to_string(),
                            review_state: state,
                            url,
                            is_technical: is_technical_rationale(r_body),
                        });
                    }
                }
            }
        }
    }

    // 2. Comments
    let comments_url =
        format!("https://api.github.com/repos/{owner_repo}/issues/{number}/comments");
    if let Ok(mut com_res) = ureq::get(&comments_url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "git-lineage")
        .header("Accept", "application/vnd.github+json")
        .call()
    {
        if let Ok(coms) = com_res.body_mut().read_json::<serde_json::Value>() {
            if let Some(com_arr) = coms.as_array() {
                for c in com_arr {
                    let c_body = c["body"].as_str().unwrap_or("").trim();
                    if c_body.len() > 10 {
                        let author = c["user"]["login"]
                            .as_str()
                            .unwrap_or("collaborator")
                            .to_string();
                        let url = c["html_url"].as_str().map(String::from);
                        reviewer_highlights.push(ReviewHighlight {
                            author,
                            body: c_body.to_string(),
                            review_state: Some("COMMENT".to_string()),
                            url,
                            is_technical: is_technical_rationale(c_body),
                        });
                    }
                }
            }
        }
    }

    Some(PrDetails {
        number,
        title,
        url: pr_url,
        body_summary,
        reviewer_highlights,
        linked_issues,
        all_matched_prs,
    })
}

/// Resolve Pull Request information, reviewer discussions, and linked issues for a commit SHA.
/// Prioritizes direct HTTPS API with token; falls back to local gh CLI session.
pub fn resolve_pr_for_commit(
    repo_root: &Path,
    remote_repo: Option<&str>,
    commit_sha: &str,
    manual_token: Option<&str>,
) -> Option<PrDetails> {
    // 1. Try Direct API with Token if owner_repo is identified
    let token = manual_token.map(String::from).or_else(resolve_github_token);
    if let (Some(tok), Some(repo)) = (&token, remote_repo) {
        if let Some(details) = fetch_github_pr_api(tok, repo, commit_sha) {
            return Some(details);
        }
    }

    // 2. Fallback to local gh CLI if present
    resolve_pr_via_gh_cli(repo_root, remote_repo, commit_sha)
}

fn resolve_pr_via_gh_cli(
    repo_root: &Path,
    remote_repo: Option<&str>,
    commit_sha: &str,
) -> Option<PrDetails> {
    let mut list_cmd = Command::new("gh");
    list_cmd.args([
        "pr",
        "list",
        "--commit",
        commit_sha,
        "--state",
        "all",
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

    // Detailed PR view with discussions, reviews, and closing issues
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

    // 3. Extract formal reviews
    if let Some(reviews) = view.reviews {
        for review in reviews {
            if let Some(body) = review.body {
                let clean = body.trim();
                if clean.len() > 10 {
                    let author = review
                        .author
                        .and_then(|a| a.login)
                        .unwrap_or_else(|| "reviewer".to_string());
                    reviewer_highlights.push(ReviewHighlight {
                        author,
                        body: clean.to_string(),
                        review_state: review.state,
                        url: review.url,
                        is_technical: is_technical_rationale(clean),
                    });
                }
            }
        }
    }

    // 4. Extract conversation comments
    if let Some(comments) = view.comments {
        for comment in comments {
            if let Some(body) = comment.body {
                let clean = body.trim();
                if clean.len() > 10 {
                    let author = comment
                        .author
                        .and_then(|a| a.login)
                        .unwrap_or_else(|| "collaborator".to_string());
                    reviewer_highlights.push(ReviewHighlight {
                        author,
                        body: clean.to_string(),
                        review_state: Some("COMMENT".to_string()),
                        url: comment.url,
                        is_technical: is_technical_rationale(clean),
                    });
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
                reviewer_highlights.push(ReviewHighlight {
                    author,
                    body: clean.to_string(),
                    review_state: Some("INLINE".to_string()),
                    url: inline.html_url,
                    is_technical: is_technical_rationale(clean),
                });
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
