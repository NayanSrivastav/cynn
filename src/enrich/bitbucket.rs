use crate::enrich::github::{is_technical_rationale, PrDetails, ReviewHighlight};
use crate::enrich::match_pr::{
    select_best_pr_match_with_audit, RejectedPrDecision, RemotePrCandidate,
};
use crate::git::{PlatformKind, RemoteInfo};

/// RFC 4648 standard base64 encoding helper
pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Discover Bitbucket API authentication token from environment
pub fn resolve_bitbucket_token() -> Option<String> {
    // 1. Direct Bearer token variables
    for var in &["BITBUCKET_TOKEN", "BITBUCKET_ACCESS_TOKEN", "BB_TOKEN"] {
        if let Ok(token) = std::env::var(var) {
            let t = token.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }

    // 2. Bitbucket App Password with Username (Basic Auth)
    if let Ok(user) = std::env::var("BITBUCKET_USER").or_else(|_| std::env::var("BB_USER")) {
        if let Ok(pw) =
            std::env::var("BITBUCKET_APP_PASSWORD").or_else(|_| std::env::var("BITBUCKET_PASSWORD"))
        {
            let u = user.trim();
            let p = pw.trim();
            if !u.is_empty() && !p.is_empty() {
                return Some(format!("{u}:{p}"));
            }
        }
    }

    None
}

/// Direct query to Bitbucket Cloud REST API v2.0
#[allow(clippy::too_many_lines)]
pub fn fetch_bitbucket_pr_api(
    token: &str,
    owner_repo: &str,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let auth_header = if token.starts_with("Basic ") || token.starts_with("Bearer ") {
        token.to_string()
    } else if token.contains(':') {
        format!("Basic {}", base64_encode(token.as_bytes()))
    } else {
        format!("Bearer {token}")
    };

    let pr_url = format!(
        "https://api.bitbucket.org/2.0/repositories/{owner_repo}/commit/{commit_sha}/pullrequests"
    );

    let Ok(mut res) = ureq::get(&pr_url)
        .header("Authorization", &auth_header)
        .header("User-Agent", "git-lineage")
        .call()
    else {
        return (None, Vec::new());
    };

    let Ok(prs_val) = res.body_mut().read_json::<serde_json::Value>() else {
        return (None, Vec::new());
    };
    let Some(arr) = prs_val["values"].as_array() else {
        return (None, Vec::new());
    };
    if arr.is_empty() {
        return (None, Vec::new());
    }

    let candidates: Vec<RemotePrCandidate> = arr
        .iter()
        .filter_map(|p| {
            let id = p["id"].as_u64()?;
            let title = p["title"].as_str().unwrap_or("").to_string();
            let url = p["links"]["html"]["href"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let author = p["author"]["nickname"]
                .as_str()
                .or_else(|| p["author"]["display_name"].as_str())
                .map(String::from);
            let description = p["summary"]["raw"].as_str().map(String::from);
            let source_branch = p["source"]["branch"]["name"].as_str().map(String::from);
            Some(RemotePrCandidate {
                id,
                title,
                description,
                source_branch,
                url,
                author,
                linked_by_platform: true,
            })
        })
        .collect();

    let all_matched_prs: Vec<u64> = candidates.iter().map(|c| c.id).collect();
    let (best, rejected) = select_best_pr_match_with_audit(
        &candidates,
        commit_subject,
        commit_body,
        PlatformKind::Bitbucket,
    );

    let Some(matched) = best else {
        return (None, rejected);
    };
    let association = matched.association.clone();
    let best = matched.candidate;

    let id = best.id;
    let title = best.title.clone();
    let url = best.url.clone();
    let author = best.author.clone();
    let raw_desc = best.description.clone();

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
        .header("Authorization", &auth_header)
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

    (
        Some(PrDetails {
            number: id,
            title,
            url,
            author,
            body_summary,
            reviewer_highlights,
            linked_issues,
            all_matched_prs,
            association,
        }),
        rejected,
    )
}

/// Resolve Bitbucket Pull Request details for a given commit
pub fn resolve_bitbucket_pr(
    _repo_root: &std::path::Path,
    remote_info: Option<&RemoteInfo>,
    commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
    manual_token: Option<&str>,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");
    let mut all_rejected = Vec::new();

    // 1. Try Direct API with Token
    let token = manual_token
        .map(String::from)
        .or_else(resolve_bitbucket_token);
    if let Some(tok) = token {
        if !owner_repo.is_empty() {
            let (details, rejected) =
                fetch_bitbucket_pr_api(&tok, owner_repo, commit_sha, commit_subject, commit_body);
            all_rejected.extend(rejected);
            if let Some(details) = details {
                return (Some(details), all_rejected);
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

                return (
                    Some(PrDetails {
                        number,
                        title: commit_subject.to_string(),
                        url,
                        author: None,
                        body_summary: None,
                        reviewer_highlights: Vec::new(),
                        linked_issues,
                        all_matched_prs: vec![number],
                        association: None,
                    }),
                    all_rejected,
                );
            }
        }
    }

    (None, all_rejected)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(
            base64_encode(b"user:app-password-123"),
            "dXNlcjphcHAtcGFzc3dvcmQtMTIz"
        );
    }
}
