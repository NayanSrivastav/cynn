use crate::enrich::match_pr::{
    select_best_pr_candidate_with_audit, RejectedPrDecision, RemotePrCandidate,
};
use crate::git::PlatformKind;
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
    pub author: Option<String>,
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
    author: Option<GhAuthor>,
    #[serde(rename = "headRefName")]
    head_ref_name: Option<String>,
}

#[derive(Deserialize)]
struct GhAuthor {
    login: Option<String>,
}

#[derive(Deserialize)]
struct GhComment {
    body: Option<String>,
    #[serde(alias = "user")]
    author: Option<GhAuthor>,
    url: Option<String>,
}

#[derive(Deserialize)]
struct GhReview {
    body: Option<String>,
    #[serde(alias = "user")]
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
    author: Option<GhAuthor>,
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

/// Heuristic filter to distinguish substantive technical review comments from conversational chatter.
///
/// NOTE: This is a pragmatic heuristic filter based on minimum length (>25 chars) and common conversational
/// phrase exclusion ("LGTM", "+1", "thanks", "approved") rather than formal semantic NLP verification.
/// Long conversational remarks may occasionally pass, while terse technical notes under 25 chars may be
/// excluded. Used to separate actionable reviewer constraints from noise.
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
    // 1. Environment variables
    if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        let t = token.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }

    // 2. gh CLI credentials command
    if let Ok(out) = Command::new("gh").args(["auth", "token"]).output() {
        if out.status.success() {
            let t = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }

    // 3. gh hosts.yml configuration file on disk
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        let candidates = [
            home.join(".config/gh/hosts.yml"),
            home.join("Library/Application Support/GitHub CLI/hosts.yml"),
        ];

        for path in &candidates {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Some(tok) = parse_gh_hosts_token(&content) {
                    return Some(tok);
                }
            }
        }
    }

    None
}

/// Parse `oauth_token` from gh hosts.yml content
pub fn parse_gh_hosts_token(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("oauth_token:") {
            let clean = rest.trim().trim_matches(|c| c == '\'' || c == '"');
            if !clean.is_empty() {
                return Some(clean.to_string());
            }
        }
    }
    None
}

/// Check if gh CLI is available on PATH
pub fn is_gh_available() -> bool {
    Command::new("gh")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Direct HTTPS query to api.github.com for commit PRs (zero CLI binary required)
#[allow(clippy::too_many_lines)]
pub fn fetch_github_pr_api(
    token: &str,
    owner_repo: &str,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let list_url = format!("https://api.github.com/repos/{owner_repo}/commits/{commit_sha}/pulls");
    let Ok(mut res) = ureq::get(&list_url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "git-lineage")
        .header("Accept", "application/vnd.github+json")
        .call()
    else {
        return (None, Vec::new());
    };

    let Ok(prs_val) = res.body_mut().read_json::<serde_json::Value>() else {
        return (None, Vec::new());
    };
    let Some(arr) = prs_val.as_array() else {
        return (None, Vec::new());
    };
    if arr.is_empty() {
        return (None, Vec::new());
    }

    let candidates: Vec<RemotePrCandidate> = arr
        .iter()
        .filter_map(|p| {
            let number = p["number"].as_u64()?;
            let title = p["title"].as_str().unwrap_or("").to_string();
            let pr_url = p["html_url"].as_str().unwrap_or("").to_string();
            let author = p["user"]["login"].as_str().map(String::from);
            let body = p["body"].as_str().map(String::from);
            let source_branch = p["head"]["ref"].as_str().map(String::from);
            Some(RemotePrCandidate {
                id: number,
                title,
                description: body,
                source_branch,
                url: pr_url,
                author,
            })
        })
        .collect();

    let all_matched_prs: Vec<u64> = candidates.iter().map(|c| c.id).collect();
    let (best, rejected) = select_best_pr_candidate_with_audit(
        &candidates,
        commit_subject,
        commit_body,
        PlatformKind::GitHub,
    );

    let Some(best) = best else {
        return (None, rejected);
    };

    let number = best.id;
    let title = best.title.clone();
    let pr_url = best.url.clone();
    let author = best.author.clone();
    let body = best.description.clone();

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

    // 1. Reviews (paginated up to 3 pages)
    for page in 1..=3 {
        let reviews_url = format!(
            "https://api.github.com/repos/{owner_repo}/pulls/{number}/reviews?per_page=100&page={page}"
        );
        let Ok(mut rev_res) = ureq::get(&reviews_url)
            .header("Authorization", &format!("Bearer {token}"))
            .header("User-Agent", "git-lineage")
            .header("Accept", "application/vnd.github+json")
            .call()
        else {
            break;
        };

        let Ok(reviews_json) = rev_res.body_mut().read_json::<serde_json::Value>() else {
            break;
        };
        let Some(rev_arr) = reviews_json.as_array() else {
            break;
        };
        if rev_arr.is_empty() {
            break;
        }
        let count = rev_arr.len();
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
        if count < 100 {
            break;
        }
    }

    // 2. Comments (paginated up to 3 pages)
    for page in 1..=3 {
        let comments_url = format!(
            "https://api.github.com/repos/{owner_repo}/issues/{number}/comments?per_page=100&page={page}"
        );
        let Ok(mut com_res) = ureq::get(&comments_url)
            .header("Authorization", &format!("Bearer {token}"))
            .header("User-Agent", "git-lineage")
            .header("Accept", "application/vnd.github+json")
            .call()
        else {
            break;
        };

        let Ok(comments_json) = com_res.body_mut().read_json::<serde_json::Value>() else {
            break;
        };
        let Some(com_arr) = comments_json.as_array() else {
            break;
        };
        if com_arr.is_empty() {
            break;
        }
        let count = com_arr.len();
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
        if count < 100 {
            break;
        }
    }

    (
        Some(PrDetails {
            number,
            title,
            url: pr_url,
            author,
            body_summary,
            reviewer_highlights,
            linked_issues,
            all_matched_prs,
        }),
        rejected,
    )
}

/// Resolve Pull Request information, reviewer discussions, and linked issues for a commit SHA.
/// Prioritizes direct HTTPS API with token; falls back to local gh CLI session.
pub fn resolve_pr_for_commit(
    repo_root: &Path,
    remote_repo: Option<&str>,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
    manual_token: Option<&str>,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let mut all_rejected = Vec::new();

    // 1. Try Direct API with Token if owner_repo is identified
    let token = manual_token.map(String::from).or_else(resolve_github_token);
    if let (Some(tok), Some(repo)) = (&token, remote_repo) {
        let (details, rejected) =
            fetch_github_pr_api(tok, repo, commit_sha, commit_subject, commit_body);
        all_rejected.extend(rejected);
        if let Some(details) = details {
            return (Some(details), all_rejected);
        }
    }

    // 2. Fallback to local gh CLI if present
    let (details, rejected) = resolve_pr_via_gh_cli(
        repo_root,
        remote_repo,
        commit_sha,
        commit_subject,
        commit_body,
    );
    for r in rejected {
        if !all_rejected.contains(&r) {
            all_rejected.push(r);
        }
    }
    (details, all_rejected)
}

#[allow(clippy::too_many_lines)]
fn resolve_pr_via_gh_cli(
    repo_root: &Path,
    remote_repo: Option<&str>,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let mut list_cmd = Command::new("gh");
    list_cmd.args([
        "pr",
        "list",
        "--commit",
        commit_sha,
        "--state",
        "all",
        "--json",
        "number,title,url,body,headRefName",
        "--limit",
        "10",
    ]);
    list_cmd.current_dir(repo_root);
    if let Some(repo) = remote_repo {
        list_cmd.args(["-R", repo]);
    }

    let list_output = match list_cmd.output() {
        Ok(out) if out.status.success() => out,
        _ => return (None, Vec::new()),
    };

    let prs: Vec<GhPrListJson> = match serde_json::from_slice(&list_output.stdout) {
        Ok(p) => p,
        Err(_) => return (None, Vec::new()),
    };
    if prs.is_empty() {
        return (None, Vec::new());
    }

    let candidates: Vec<RemotePrCandidate> = prs
        .iter()
        .map(|p| RemotePrCandidate {
            id: p.number,
            title: p.title.clone(),
            description: p.body.clone(),
            source_branch: p.head_ref_name.clone(),
            url: p.url.clone(),
            author: p.author.as_ref().and_then(|a| a.login.clone()),
        })
        .collect();

    let all_matched_prs: Vec<u64> = candidates.iter().map(|c| c.id).collect();
    let (matched_pr, rejected) = select_best_pr_candidate_with_audit(
        &candidates,
        commit_subject,
        commit_body,
        PlatformKind::GitHub,
    );

    let Some(matched_pr) = matched_pr else {
        return (None, rejected);
    };

    // Detailed PR view with discussions, reviews, and closing issues
    let mut view_cmd = Command::new("gh");
    view_cmd.args([
        "pr",
        "view",
        &matched_pr.id.to_string(),
        "--json",
        "number,title,url,body,author,comments,reviews,closingIssuesReferences",
    ]);
    view_cmd.current_dir(repo_root);
    if let Some(repo) = remote_repo {
        view_cmd.args(["-R", repo]);
    }

    let mut inline_comments = Vec::new();
    if let Some(repo) = remote_repo {
        let endpoint = format!("repos/{repo}/pulls/{}/comments", matched_pr.id);
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
                return (
                    Some(build_pr_details(details, inline_comments, all_matched_prs)),
                    rejected,
                );
            }
        }
    }

    // Fallback if detailed pr view fails
    let body_summary = matched_pr.description.as_ref().and_then(|b| {
        let trimmed = b.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.lines().next().unwrap_or("").trim().to_string())
        }
    });

    let linked_issues = matched_pr
        .description
        .as_deref()
        .map(extract_issues_from_text)
        .unwrap_or_default();

    (
        Some(PrDetails {
            number: matched_pr.id,
            title: matched_pr.title.clone(),
            url: matched_pr.url.clone(),
            author: matched_pr.author.clone(),
            body_summary,
            reviewer_highlights: Vec::new(),
            linked_issues,
            all_matched_prs,
        }),
        rejected,
    )
}

#[allow(clippy::too_many_lines)]
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
        author: view.author.and_then(|a| a.login),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gh_hosts_token() {
        let sample = r"
github.com:
    user: testuser
    oauth_token: gho_secrettoken12345
    git_protocol: ssh
";
        assert_eq!(
            parse_gh_hosts_token(sample),
            Some("gho_secrettoken12345".to_string())
        );
    }

    #[test]
    fn test_parse_gh_pr_view_merged() {
        let json_data = r#"{
            "number": 42,
            "title": "feat: merged feature PR",
            "url": "https://github.com/acme/repo/pull/42",
            "body": "This is a substantive merged PR description explaining the technical rationale.",
            "author": {"login": "octocat"},
            "state": "MERGED",
            "comments": [
                {
                    "body": "LGTM",
                    "author": {"login": "reviewer1"}
                },
                {
                    "body": "Ensure this handles edge case where count is zero.",
                    "author": {"login": "techlead"},
                    "url": "https://github.com/acme/repo/pull/42#comment-1"
                }
            ],
            "reviews": [],
            "closingIssuesReferences": [
                {"number": 101}
            ]
        }"#;

        let view: GhPrViewJson = serde_json::from_str(json_data).unwrap();
        let pr = build_pr_details(view, vec![], vec![42]);

        assert_eq!(pr.number, 42);
        assert_eq!(pr.author.as_deref(), Some("octocat"));
        assert_eq!(pr.linked_issues, vec!["#101"]);
        assert!(pr.body_summary.is_some());
        assert_eq!(pr.reviewer_highlights.len(), 1); // "LGTM" filtered out
        assert_eq!(pr.reviewer_highlights[0].author, "techlead");
        assert!(pr.reviewer_highlights[0].is_technical);
    }

    #[test]
    fn test_parse_gh_pr_list_merged_and_closed() {
        let json_data = r#"[
            {
                "number": 104,
                "title": "fix: merged hotfix",
                "url": "https://github.com/acme/repo/pull/104",
                "body": "Hotfix merged into main."
            },
            {
                "number": 99,
                "title": "chore: old closed PR",
                "url": "https://github.com/acme/repo/pull/99",
                "body": "Closed without merge."
            }
        ]"#;

        let prs: Vec<GhPrListJson> = serde_json::from_str(json_data).unwrap();
        assert_eq!(prs.len(), 2);
        assert_eq!(prs[0].number, 104);
        assert_eq!(prs[1].number, 99);
    }
}
