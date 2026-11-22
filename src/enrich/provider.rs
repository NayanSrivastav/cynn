use crate::enrich::bitbucket::resolve_bitbucket_pr;
use crate::enrich::github::{resolve_pr_for_commit as resolve_github_pr, PrDetails};
use crate::enrich::gitlab::resolve_gitlab_mr;
use crate::git::{CommitInfo, PlatformKind, RemoteInfo};
use std::path::Path;

/// Resolve remote context (PR, MR, issues, discussions) across GitHub, GitLab, Bitbucket, and Generic Git
pub fn resolve_remote_context(
    repo_root: &Path,
    remote_info: Option<&RemoteInfo>,
    commit: &CommitInfo,
) -> Option<PrDetails> {
    let platform = remote_info
        .map(|r| r.platform)
        .unwrap_or(PlatformKind::Generic);

    match platform {
        PlatformKind::GitHub => {
            let owner_repo = remote_info.map(|r| r.owner_repo.as_str());
            if let Some(pr) = resolve_github_pr(repo_root, owner_repo, &commit.sha) {
                return Some(pr);
            }
            // Fallback to commit trailers if gh CLI returns no match
            resolve_generic_trailer(remote_info, commit)
        }
        PlatformKind::GitLab => {
            if let Some(mr) = resolve_gitlab_mr(
                repo_root,
                remote_info,
                &commit.sha,
                &commit.subject,
                &commit.body,
            ) {
                return Some(mr);
            }
            resolve_generic_trailer(remote_info, commit)
        }
        PlatformKind::Bitbucket => {
            if let Some(pr) = resolve_bitbucket_pr(
                repo_root,
                remote_info,
                &commit.sha,
                &commit.subject,
                &commit.body,
            ) {
                return Some(pr);
            }
            resolve_generic_trailer(remote_info, commit)
        }
        PlatformKind::Generic => resolve_generic_trailer(remote_info, commit),
    }
}

/// Offline / generic commit trailer parser for universal Git repositories
fn resolve_generic_trailer(
    remote_info: Option<&RemoteInfo>,
    commit: &CommitInfo,
) -> Option<PrDetails> {
    let full_text = format!("{}\n{}", commit.subject, commit.body);

    // Look for common patterns:
    // 1. Merge pull request #123
    // 2. See merge request !123
    // 3. Merged in ... (pull request #123)
    // 4. (#123)
    for word in full_text.split_whitespace() {
        let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '#' && c != '!');
        if let Some(num_str) = clean.strip_prefix('#').or_else(|| clean.strip_prefix('!')) {
            if let Ok(number) = num_str.parse::<u64>() {
                if number > 0 {
                    let host = remote_info.map(|r| r.host.as_str()).unwrap_or("git");
                    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");
                    let url = format!("https://{host}/{owner_repo}/pull/{number}");

                    return Some(PrDetails {
                        number,
                        title: commit.subject.clone(),
                        url,
                        body_summary: None,
                        reviewer_highlights: Vec::new(),
                        linked_issues: extract_trailer_issues(&full_text),
                        all_matched_prs: vec![number],
                    });
                }
            }
        }
    }

    None
}

fn extract_trailer_issues(text: &str) -> Vec<String> {
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
