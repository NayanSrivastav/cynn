use crate::enrich::github::PrDetails;
use crate::git::RemoteInfo;

/// Resolve Bitbucket Pull Request details for a given commit
pub fn resolve_bitbucket_pr(
    _repo_root: &std::path::Path,
    remote_info: Option<&RemoteInfo>,
    _commit_sha: &str,
    commit_subject: &str,
    commit_body: &str,
) -> Option<PrDetails> {
    // Bitbucket standard merge commit pattern:
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
                let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");
                let url = format!("https://{host}/{owner_repo}/pull-requests/{number}");

                let body_summary = None;

                let linked_issues = extract_bitbucket_issues(&full_text);

                return Some(PrDetails {
                    number,
                    title: commit_subject.to_string(),
                    url,
                    body_summary,
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
