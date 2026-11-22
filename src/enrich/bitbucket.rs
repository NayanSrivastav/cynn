use crate::enrich::github::{is_technical_rationale, PrDetails, ReviewHighlight};
use crate::git::RemoteInfo;

/// Discover Bitbucket API authentication token
pub fn resolve_bitbucket_token() -> Option<String> {
    if let Ok(token) = std::env::var("BITBUCKET_TOKEN") {
        let t = token.trim();
        if !t.is_empty() {
            return Some(t.to_string());
        }
    }
    None
}

/// Direct query to Bitbucket Cloud REST API v2.0
pub fn fetch_bitbucket_pr_api(
    token: &str,
    owner_repo: &str,
    commit_sha: &str,
) -> Option<PrDetails> {
    let pr_url = format!(
        "https://api.bitbucket.org/2.0/repositories/{owner_repo}/commit/{commit_sha}/pullrequests"
    );

    let mut res = ureq::get(&pr_url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "git-lineage")
        .call()
        .ok()?;

    let prs_val: serde_json::Value = res.body_mut().read_json().ok()?;
    let arr = prs_val["values"].as_array()?;
    if arr.is_empty() {
        return None;
    }

    let first = &arr[0];
    let id = first["id"].as_u64()?;
    let title = first["title"].as_str().unwrap_or("").to_string();
    let url = first["links"]["html"]["href"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let raw_desc = first["summary"]["raw"].as_str().map(String::from);

    let body_summary = raw_desc.as_ref().and_then(|d| {
        let trimmed = d.trim();
        if trimmed.is_empty() {
            None
        } else {
            let first_para = trimmed.split("\n\n").next().unwrap_or(trimmed).trim();
            Some(first_para.replace('\n', " "))
        }
    });

    let mut linked_issues = Vec::new();
    if let Some(ref d) = raw_desc {
        for issue in extract_bitbucket_issues(d) {
            if !linked_issues.contains(&issue) {
                linked_issues.push(issue);
            }
        }
    }

    let mut reviewer_highlights = Vec::new();

    // Fetch comments
    let comments_url = format!(
        "https://api.bitbucket.org/2.0/repositories/{owner_repo}/pullrequests/{id}/comments"
    );
    if let Ok(mut com_res) = ureq::get(&comments_url)
        .header("Authorization", &format!("Bearer {token}"))
        .header("User-Agent", "git-lineage")
        .call()
    {
        if let Ok(com_val) = com_res.body_mut().read_json::<serde_json::Value>() {
            if let Some(com_arr) = com_val["values"].as_array() {
                for c in com_arr {
                    let c_body = c["content"]["raw"].as_str().unwrap_or("").trim();
                    if c_body.len() > 10 {
                        let author = c["user"]["display_name"]
                            .as_str()
                            .unwrap_or("reviewer")
                            .to_string();
                        reviewer_highlights.push(ReviewHighlight {
                            author,
                            body: c_body.to_string(),
                            review_state: Some("COMMENT".to_string()),
                            url: None,
                            is_technical: is_technical_rationale(c_body),
                        });
                    }
                }
            }
        }
    }

    Some(PrDetails {
        number: id,
        title,
        url,
        body_summary,
        reviewer_highlights,
        linked_issues,
        all_matched_prs: vec![id],
    })
}

/// Resolve Bitbucket Pull Request details for a given commit
pub fn resolve_bitbucket_pr(
    _repo_root: &std::path::Path,
    remote_info: Option<&RemoteInfo>,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
    manual_token: Option<&str>,
) -> Option<PrDetails> {
    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");

    // 1. Try Direct API with Token
    let token = manual_token
        .map(String::from)
        .or_else(resolve_bitbucket_token);
    if let Some(tok) = token {
        if !owner_repo.is_empty() {
            if let Some(details) = fetch_bitbucket_pr_api(&tok, owner_repo, commit_sha) {
                return Some(details);
            }
        }
    }

    // 2. Fallback to Bitbucket standard merge commit pattern:
    // "Merged in feature_branch (pull request #42)"
    let full_text = format!("{commit_subject}\n{commit_body}");
    for line in full_text.lines() {
        if let Some(idx) = line.find("pull request #") {
            let num_str: String = line[idx + "pull request #".len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(number) = num_str.parse::<u64>() {
                let host = remote_info
                    .map(|r| r.host.as_str())
                    .unwrap_or("bitbucket.org");
                let url = format!("https://{host}/{owner_repo}/pull-requests/{number}");

                let linked_issues = extract_bitbucket_issues(&full_text);

                return Some(PrDetails {
                    number,
                    title: commit_subject.to_string(),
                    url,
                    body_summary: None,
                    reviewer_highlights: Vec::new(),
                    linked_issues,
                    all_matched_prs: vec![number],
                });
            }
        }
    }

    None
}

fn extract_bitbucket_issues(text: &str) -> Vec<String> {
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
