use crate::enrich::github::PrDetails;
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

/// Resolve GitLab Merge Request details for a given commit
pub fn resolve_gitlab_mr(
    repo_root: &Path,
    remote_info: Option<&RemoteInfo>,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
) -> Option<PrDetails> {
    // 1. Try resolving via local `glab` CLI if available
    let mut cmd = Command::new("glab");
    cmd.args(["mr", "list", "--commit", commit_sha, "--output", "json"]);
    cmd.current_dir(repo_root);

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            if let Ok(mrs) = serde_json::from_slice::<Vec<GlabMrJson>>(&output.stdout) {
                if let Some(mr) = mrs.into_iter().next() {
                    let host = remote_info.map(|r| r.host.as_str()).unwrap_or("gitlab.com");
                    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");
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
                        body_summary,
                        reviewer_highlights: Vec::new(),
                        linked_issues,
                        all_matched_prs: vec![mr.iid],
                    });
                }
            }
        }
    }

    // 2. Offline fallback: Parse standard GitLab MR commit trailers
    // Pattern: "See merge request !123" or "See merge request group/project!123"
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
