use crate::enrich::github::{is_technical_rationale, PrDetails, ReviewHighlight};
use crate::git::RemoteInfo;
use serde::Deserialize;
use std::path::Path;
use std::process::Command;

#[derive(Deserialize)]
struct GlabMrJson {
    iid: u64,
    title: String,
    web_url: Option<String>,
    description: Option<String>,
}

/// Discover GitLab API authentication token
pub fn resolve_gitlab_token() -> Option<String> {
    if let Ok(token) = std::env::var("GITLAB_TOKEN") {
        let t = token.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// Direct query to GitLab v4 REST API
pub fn fetch_gitlab_mr_api(
    host: &str,
    token: &str,
    owner_repo: &str,
    commit_sha: &str,
) -> Option<PrDetails> {
    let encoded_repo = owner_repo.replace('/', "%2F");
    let mr_url = format!(
        "https://{host}/api/v4/projects/{encoded_repo}/repository/commits/{commit_sha}/merge_requests"
    );

    let mut res = ureq::get(&mr_url)
        .header("PRIVATE-TOKEN", token)
        .header("User-Agent", "git-lineage")
        .call()
        .ok()?;

    let mrs_val: serde_json::Value = res.body_mut().read_json().ok()?;
    let arr = mrs_val.as_array()?;
    if arr.is_empty() {
        return None;
    }

    let first = &arr[0];
    let iid = first["iid"].as_u64()?;
    let title = first["title"].as_str().unwrap_or("").to_string();
    let url = first["web_url"]
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| format!("https://{host}/{owner_repo}/-/merge_requests/{iid}"));
    let author = first["author"]["username"].as_str().map(String::from);
    let description = first["description"].as_str().map(String::from);

    let body_summary = description.as_ref().and_then(|d| {
        let trimmed = d.trim();
        if trimmed.is_empty() {
            None
        } else {
            let first_para = trimmed.split("\n\n").next().unwrap_or(trimmed).trim();
            Some(first_para.replace('\n', " "))
        }
    });

    let mut linked_issues = Vec::new();
    if let Some(ref d) = description {
        for issue in extract_gitlab_issues(d) {
            if !linked_issues.contains(&issue) {
                linked_issues.push(issue);
            }
        }
    }

    let mut reviewer_highlights = Vec::new();

    // Fetch MR discussions/notes
    let notes_url = format!(
        "https://{host}/api/v4/projects/{encoded_repo}/merge_requests/{iid}/notes?per_page=20"
    );
    if let Ok(mut notes_res) = ureq::get(&notes_url)
        .header("PRIVATE-TOKEN", token)
        .header("User-Agent", "git-lineage")
        .call()
    {
        if let Ok(notes_val) = notes_res.body_mut().read_json::<serde_json::Value>() {
            if let Some(notes_arr) = notes_val.as_array() {
                for n in notes_arr {
                    let n_body = n["body"].as_str().unwrap_or("").trim();
                    let system = n["system"].as_bool().unwrap_or(false);
                    if !system && n_body.len() > 10 {
                        let author = n["author"]["username"]
                            .as_str()
                            .unwrap_or("collaborator")
                            .to_string();
                        reviewer_highlights.push(ReviewHighlight {
                            author,
                            body: n_body.to_string(),
                            review_state: Some("NOTE".to_string()),
                            url: None,
                            is_technical: is_technical_rationale(n_body),
                        });
                    }
                }
            }
        }
    }

    Some(PrDetails {
        number: iid,
        title,
        url,
        author,
        body_summary,
        reviewer_highlights,
        linked_issues,
        all_matched_prs: vec![iid],
    })
}

/// Resolve GitLab Merge Request details for a given commit
pub fn resolve_gitlab_mr(
    repo_root: &Path,
    remote_info: Option<&RemoteInfo>,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
    manual_token: Option<&str>,
) -> Option<PrDetails> {
    let host = remote_info.map(|r| r.host.as_str()).unwrap_or("gitlab.com");
    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");

    // 1. Try Direct API with Token
    let token = manual_token.map(String::from).or_else(resolve_gitlab_token);
    if let Some(tok) = token {
        if !owner_repo.is_empty() {
            if let Some(details) = fetch_gitlab_mr_api(host, &tok, owner_repo, commit_sha) {
                return Some(details);
            }
        }
    }

    // 2. Try resolving via local `glab` CLI if available
    let mut cmd = Command::new("glab");
    cmd.args(["mr", "list", "--commit", commit_sha, "--output", "json"]);
    cmd.current_dir(repo_root);

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            if let Ok(mrs) = serde_json::from_slice::<Vec<GlabMrJson>>(&output.stdout) {
                if let Some(mr) = mrs.into_iter().next() {
                    let url = mr.web_url.unwrap_or_else(|| {
                        format!("https://{host}/{owner_repo}/-/merge_requests/{}", mr.iid)
                    });

                    let body_summary = mr.description.as_ref().and_then(|d| {
                        let trimmed = d.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.lines().next().unwrap_or("").trim().to_string())
                        }
                    });

                    let linked_issues = mr
                        .description
                        .as_deref()
                        .map(extract_gitlab_issues)
                        .unwrap_or_default();

                    return Some(PrDetails {
                        number: mr.iid,
                        title: mr.title,
                        url,
                        author: None,
                        body_summary,
                        reviewer_highlights: Vec::new(),
                        linked_issues,
                        all_matched_prs: vec![mr.iid],
                    });
                }
            }
        }
    }

    // 3. Fallback to commit message trailers
    parse_gitlab_trailer(remote_info, commit_subject, commit_body)
}

fn parse_gitlab_trailer(
    remote_info: Option<&RemoteInfo>,
    subject: &str,
    body: &str,
) -> Option<PrDetails> {
    let full_text = format!("{subject}\n{body}");
    for line in full_text.lines() {
        if let Some(idx) = line.find("See merge request") {
            let rest = line[idx + "See merge request".len()..].trim();
            if let Some(bang_idx) = rest.find('!') {
                let num_str: String = rest[bang_idx + 1..]
                    .chars()
                    .take_while(char::is_ascii_digit)
                    .collect();
                if let Ok(number) = num_str.parse::<u64>() {
                    let host = remote_info.map(|r| r.host.as_str()).unwrap_or("gitlab.com");
                    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");
                    let url = format!("https://{host}/{owner_repo}/-/merge_requests/{number}");

                    return Some(PrDetails {
                        number,
                        title: subject.to_string(),
                        url,
                        author: None,
                        body_summary: None,
                        reviewer_highlights: Vec::new(),
                        linked_issues: extract_gitlab_issues(&full_text),
                        all_matched_prs: vec![number],
                    });
                }
            }
        }
    }

    None
}

fn extract_gitlab_issues(text: &str) -> Vec<String> {
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
