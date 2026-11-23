use crate::enrich::github::{is_technical_rationale, PrDetails, ReviewHighlight};
use crate::enrich::match_pr::{
    select_best_pr_candidate_with_audit, RejectedPrDecision, RemotePrCandidate,
};
use crate::git::{PlatformKind, RemoteInfo};
use std::path::Path;
use std::process::Command;

/// Parse token from glab config.yml content
pub fn parse_glab_config_token(content: &str, target_host: Option<&str>) -> Option<String> {
    let host_to_match = target_host.unwrap_or("gitlab.com");
    let mut in_hosts_section = false;
    let mut in_target_host = false;
    let mut fallback_token: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "hosts:" {
            in_hosts_section = true;
            continue;
        }

        if in_hosts_section {
            if trimmed.ends_with(':') && !trimmed.starts_with("token:") && !trimmed.starts_with('#')
            {
                let host_name = trimmed.trim_end_matches(':').trim();
                in_target_host = host_name == host_to_match;
            }

            if let Some(rest) = trimmed.strip_prefix("token:") {
                let mut val = rest.trim();
                if let Some(after_null) = val.strip_prefix("!!null") {
                    val = after_null.trim();
                }
                let clean = val.trim_matches(|c| c == '\'' || c == '"').to_string();
                if !clean.is_empty() && clean != "null" {
                    if in_target_host {
                        return Some(clean);
                    } else if fallback_token.is_none() {
                        fallback_token = Some(clean);
                    }
                }
            }
        }
    }

    fallback_token
}

/// Discover GitLab API authentication token from environment or glab CLI config.yml
pub fn resolve_gitlab_token(target_host: Option<&str>) -> Option<String> {
    // 1. Environment variables
    if let Ok(token) = std::env::var("GITLAB_TOKEN").or_else(|_| std::env::var("GL_TOKEN")) {
        let t = token.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }

    // 2. glab CLI config file
    if let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) {
        let candidates = [
            home.join("Library/Application Support/glab-cli/config.yml"),
            home.join(".config/glab-cli/config.yml"),
        ];

        for path in &candidates {
            if let Ok(content) = std::fs::read_to_string(path) {
                if let Some(tok) = parse_glab_config_token(&content, target_host) {
                    return Some(tok);
                }
            }
        }
    }

    None
}

/// Check if glab CLI is available on PATH
pub fn is_glab_available() -> bool {
    Command::new("glab")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Direct query to GitLab v4 REST API
#[allow(clippy::too_many_lines)]
pub fn fetch_gitlab_mr_api(
    host: &str,
    token: &str,
    owner_repo: &str,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let encoded_repo = owner_repo.replace('/', "%2F");
    let mr_url = format!(
        "https://{host}/api/v4/projects/{encoded_repo}/repository/commits/{commit_sha}/merge_requests"
    );

    let Ok(mut res) = ureq::get(&mr_url)
        .header("PRIVATE-TOKEN", token)
        .header("User-Agent", "git-lineage")
        .call()
    else {
        return (None, Vec::new());
    };

    let Ok(mrs_val) = res.body_mut().read_json::<serde_json::Value>() else {
        return (None, Vec::new());
    };
    let Some(arr) = mrs_val.as_array() else {
        return (None, Vec::new());
    };
    if arr.is_empty() {
        return (None, Vec::new());
    }

    let candidates: Vec<RemotePrCandidate> =
        arr.iter()
            .filter_map(|m| {
                let iid = m["iid"].as_u64()?;
                let title = m["title"].as_str().unwrap_or("").to_string();
                let web_url = m["web_url"].as_str().map(String::from).unwrap_or_else(|| {
                    format!("https://{host}/{owner_repo}/-/merge_requests/{iid}")
                });
                let description = m["description"].as_str().map(String::from);
                let source_branch = m["source_branch"].as_str().map(String::from);
                let author = m["author"]["username"].as_str().map(String::from);
                Some(RemotePrCandidate {
                    id: iid,
                    title,
                    description,
                    source_branch,
                    url: web_url,
                    author,
                })
            })
            .collect();

    let all_matched_prs: Vec<u64> = candidates.iter().map(|c| c.id).collect();
    let (best, rejected) = select_best_pr_candidate_with_audit(
        &candidates,
        commit_subject,
        commit_body,
        PlatformKind::GitLab,
    );

    let Some(best) = best else {
        return (None, rejected);
    };

    let iid = best.id;
    let title = best.title.clone();
    let url = best.url.clone();
    let author = best.author.clone();
    let description = best.description.clone();

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

    (
        Some(PrDetails {
            number: iid,
            title,
            url,
            author,
            body_summary,
            reviewer_highlights,
            linked_issues,
            all_matched_prs,
        }),
        rejected,
    )
}

/// Resolve GitLab Merge Request details for a given commit
#[allow(clippy::too_many_lines)]
pub fn resolve_gitlab_mr(
    repo_root: &Path,
    remote_info: Option<&RemoteInfo>,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
    manual_token: Option<&str>,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let host = remote_info.map(|r| r.host.as_str()).unwrap_or("gitlab.com");
    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");
    let mut all_rejected = Vec::new();

    // 1. Try Direct API with Token
    let token = manual_token
        .map(String::from)
        .or_else(|| resolve_gitlab_token(Some(host)));
    if let Some(tok) = token {
        if !owner_repo.is_empty() {
            let (details, rejected) = fetch_gitlab_mr_api(
                host,
                &tok,
                owner_repo,
                commit_sha,
                commit_subject,
                commit_body,
            );
            all_rejected.extend(rejected);
            if let Some(details) = details {
                return (Some(details), all_rejected);
            }
        }
    }

    // 2. Try resolving via local `glab` CLI if available
    let mut cmd = Command::new("glab");
    cmd.args([
        "api",
        &format!("projects/:id/repository/commits/{commit_sha}/merge_requests"),
    ]);
    cmd.current_dir(repo_root);

    if let Ok(output) = cmd.output() {
        if output.status.success() {
            if let Ok(mrs_val) = serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                if let Some(arr) = mrs_val.as_array() {
                    let candidates: Vec<RemotePrCandidate> = arr
                        .iter()
                        .filter_map(|m| {
                            let iid = m["iid"].as_u64()?;
                            let title = m["title"].as_str().unwrap_or("").to_string();
                            let web_url =
                                m["web_url"].as_str().map(String::from).unwrap_or_else(|| {
                                    format!("https://{host}/{owner_repo}/-/merge_requests/{iid}")
                                });
                            let description = m["description"].as_str().map(String::from);
                            let source_branch = m["source_branch"].as_str().map(String::from);
                            let author = m["author"]["username"].as_str().map(String::from);
                            Some(RemotePrCandidate {
                                id: iid,
                                title,
                                description,
                                source_branch,
                                url: web_url,
                                author,
                            })
                        })
                        .collect();

                    let all_matched_prs: Vec<u64> = candidates.iter().map(|c| c.id).collect();
                    let (best, rejected) = select_best_pr_candidate_with_audit(
                        &candidates,
                        commit_subject,
                        commit_body,
                        PlatformKind::GitLab,
                    );
                    for r in rejected {
                        if !all_rejected.contains(&r) {
                            all_rejected.push(r);
                        }
                    }

                    if let Some(best) = best {
                        let body_summary = best.description.as_ref().and_then(|d| {
                            let trimmed = d.trim();
                            if trimmed.is_empty() {
                                None
                            } else {
                                Some(trimmed.lines().next().unwrap_or("").trim().to_string())
                            }
                        });

                        let linked_issues = best
                            .description
                            .as_deref()
                            .map(extract_gitlab_issues)
                            .unwrap_or_default();

                        return (
                            Some(PrDetails {
                                number: best.id,
                                title: best.title.clone(),
                                url: best.url.clone(),
                                author: best.author.clone(),
                                body_summary,
                                reviewer_highlights: Vec::new(),
                                linked_issues,
                                all_matched_prs,
                            }),
                            all_rejected,
                        );
                    }
                }
            }
        }
    }

    // 3. Fallback to commit message trailers
    (
        parse_gitlab_trailer(remote_info, commit_subject, commit_body),
        all_rejected,
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_glab_config_token_standard() {
        let sample = r"
hosts:
    gitlab.com:
        token: glpat-sample123
        user: testuser
";
        assert_eq!(
            parse_glab_config_token(sample, Some("gitlab.com")),
            Some("glpat-sample123".to_string())
        );
    }

    #[test]
    fn test_parse_glab_config_token_with_yaml_null_tag() {
        let sample = r"
hosts:
    gitlab.com:
        token: !!null glpat-ehC--UJGUPvuLKvYFADNKmM6MQpvOjEKdTo3OXI0dQ8
        git_protocol: ssh
";
        assert_eq!(
            parse_glab_config_token(sample, Some("gitlab.com")),
            Some("glpat-ehC--UJGUPvuLKvYFADNKmM6MQpvOjEKdTo3OXI0dQ8".to_string())
        );
    }

    #[test]
    fn test_parse_glab_config_token_custom_host() {
        let sample = r#"
hosts:
    gitlab.example.org:
        token: "glpat-corp999"
    gitlab.com:
        token: glpat-com111
"#;
        assert_eq!(
            parse_glab_config_token(sample, Some("gitlab.example.org")),
            Some("glpat-corp999".to_string())
        );
        assert_eq!(
            parse_glab_config_token(sample, Some("gitlab.com")),
            Some("glpat-com111".to_string())
        );
    }
}
