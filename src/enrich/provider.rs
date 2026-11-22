use crate::enrich::bitbucket::resolve_bitbucket_pr;
use crate::enrich::github::{resolve_pr_for_commit as resolve_github_pr, PrDetails};
use crate::enrich::gitlab::resolve_gitlab_mr;
use crate::git::{CommitInfo, PlatformKind, RemoteInfo};
use std::io::IsTerminal;
use std::path::Path;

/// Resolve remote context (PR, MR, issues, discussions) across GitHub, GitLab, Bitbucket, and Generic Git.
/// Supports both Direct HTTPS APIs with Token, local CLI sessions (gh/glab), and offline Git trailers.
pub fn resolve_remote_context(
    repo_root: &Path,
    remote_info: Option<&RemoteInfo>,
    commit: &CommitInfo,
) -> Option<PrDetails> {
    let platform = remote_info
        .map(|r| r.platform)
        .unwrap_or(PlatformKind::Generic);

    let manual_token = check_or_prompt_token(remote_info);

    match platform {
        PlatformKind::GitHub => {
            let owner_repo = remote_info.map(|r| r.owner_repo.as_str());
            if let Some(pr) =
                resolve_github_pr(repo_root, owner_repo, &commit.sha, manual_token.as_deref())
            {
                return Some(pr);
            }
            resolve_generic_trailer(remote_info, commit)
        }
        PlatformKind::GitLab => {
            if let Some(mr) = resolve_gitlab_mr(
                repo_root,
                remote_info,
                &commit.sha,
                &commit.subject,
                &commit.body,
                manual_token.as_deref(),
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
                manual_token.as_deref(),
            ) {
                return Some(pr);
            }
            resolve_generic_trailer(remote_info, commit)
        }
        PlatformKind::Generic => resolve_generic_trailer(remote_info, commit),
    }
}

/// Check environment for tokens; if none exist and terminal is interactive, prompt the user.
fn check_or_prompt_token(remote_info: Option<&RemoteInfo>) -> Option<String> {
    let info = remote_info?;
    match info.platform {
        PlatformKind::GitHub => {
            if let Some(tok) = crate::enrich::github::resolve_github_token() {
                return Some(tok);
            }
            prompt_interactive_token("GitHub", "GITHUB_TOKEN", &info.owner_repo)
        }
        PlatformKind::GitLab => {
            if let Some(tok) = crate::enrich::gitlab::resolve_gitlab_token() {
                return Some(tok);
            }
            prompt_interactive_token("GitLab", "GITLAB_TOKEN", &info.owner_repo)
        }
        PlatformKind::Bitbucket => {
            if let Some(tok) = crate::enrich::bitbucket::resolve_bitbucket_token() {
                return Some(tok);
            }
            prompt_interactive_token("Bitbucket", "BITBUCKET_TOKEN", &info.owner_repo)
        }
        PlatformKind::Generic => None,
    }
}

fn prompt_interactive_token(platform_name: &str, env_var: &str, repo_name: &str) -> Option<String> {
    if cfg!(test)
        || std::env::var("CARGO").is_ok()
        || !std::io::stdin().is_terminal()
        || repo_name.is_empty()
    {
        return None;
    }

    eprintln!("💡 Tip: No {platform_name} token or CLI detected for {repo_name}.");
    eprint!("   Enter {env_var} to enrich PR discussions (or press Enter to proceed offline): ");
    let _ = std::io::Write::flush(&mut std::io::stderr());

    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_ok() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Offline / generic commit trailer parser for universal Git repositories
fn resolve_generic_trailer(
    remote_info: Option<&RemoteInfo>,
    commit: &CommitInfo,
) -> Option<PrDetails> {
    let full_text = format!("{}\n{}", commit.subject, commit.body);

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
                        author: None,
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
