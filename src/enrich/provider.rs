use crate::enrich::bitbucket::resolve_bitbucket_pr;
use crate::enrich::github::{resolve_pr_for_commit as resolve_github_pr, PrDetails};
use crate::enrich::gitlab::resolve_gitlab_mr;
use crate::enrich::match_pr::RejectedPrDecision;
use crate::git::{CommitInfo, PlatformKind, RemoteInfo};
use std::io::IsTerminal;
use std::path::Path;

use std::collections::HashMap;
use std::sync::Mutex;

type RemoteCacheResult = (Option<PrDetails>, Vec<RejectedPrDecision>);
static COMMIT_REMOTE_CACHE: Mutex<Option<HashMap<String, RemoteCacheResult>>> = Mutex::new(None);

/// Resolve remote context (PR, MR, issues, discussions) across GitHub, GitLab, Bitbucket, and Generic Git.
/// Supports both Direct HTTPS APIs with Token, local CLI sessions (gh/glab), and offline Git trailers.
pub fn resolve_remote_context(
    repo_root: &Path,
    remote_info: Option<&RemoteInfo>,
    commit: &CommitInfo,
) -> (Option<PrDetails>, Vec<RejectedPrDecision>) {
    let platform = remote_info
        .map(|r| r.platform)
        .unwrap_or(PlatformKind::Generic);

    let host = remote_info.map(|r| r.host.as_str()).unwrap_or("local");
    let owner_repo = remote_info.map(|r| r.owner_repo.as_str()).unwrap_or("");
    let cache_key = format!("{platform:?}:{host}:{owner_repo}:{}", commit.sha);

    if let Ok(guard) = COMMIT_REMOTE_CACHE.lock() {
        if let Some(map) = guard.as_ref() {
            if let Some(cached) = map.get(&cache_key) {
                return cached.clone();
            }
        }
    }

    let manual_token = check_or_prompt_token(remote_info);

    let res = match platform {
        PlatformKind::GitHub => {
            let owner_repo = remote_info.map(|r| r.owner_repo.as_str());
            let (pr, rejected) = resolve_github_pr(
                repo_root,
                owner_repo,
                &commit.sha,
                &commit.subject,
                &commit.body,
                manual_token.as_deref(),
            );
            if let Some(pr) = pr {
                (Some(pr), rejected)
            } else {
                (resolve_generic_trailer(remote_info, commit), rejected)
            }
        }
        PlatformKind::GitLab => {
            let (mr, rejected) = resolve_gitlab_mr(
                repo_root,
                remote_info,
                &commit.sha,
                &commit.subject,
                &commit.body,
                manual_token.as_deref(),
            );
            if let Some(mr) = mr {
                (Some(mr), rejected)
            } else {
                (resolve_generic_trailer(remote_info, commit), rejected)
            }
        }
        PlatformKind::Bitbucket => {
            let (pr, rejected) = resolve_bitbucket_pr(
                repo_root,
                remote_info,
                &commit.sha,
                &commit.subject,
                &commit.body,
                manual_token.as_deref(),
            );
            if let Some(pr) = pr {
                (Some(pr), rejected)
            } else {
                (resolve_generic_trailer(remote_info, commit), rejected)
            }
        }
        PlatformKind::Generic => (resolve_generic_trailer(remote_info, commit), Vec::new()),
    };

    if let Ok(mut guard) = COMMIT_REMOTE_CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        map.insert(cache_key, res.clone());
    }

    res
}

/// Check environment for tokens; if none exist and terminal is interactive, prompt the user.
fn check_or_prompt_token(remote_info: Option<&RemoteInfo>) -> Option<String> {
    let info = remote_info?;
    match info.platform {
        PlatformKind::GitHub => {
            if let Some(tok) = crate::enrich::github::resolve_github_token() {
                return Some(tok);
            }
            if crate::enrich::github::is_gh_available() {
                return None;
            }
            get_cached_or_prompt("GitHub", "GITHUB_TOKEN", &info.owner_repo)
        }
        PlatformKind::GitLab => {
            if let Some(tok) = crate::enrich::gitlab::resolve_gitlab_token(Some(&info.host)) {
                return Some(tok);
            }
            if crate::enrich::gitlab::is_glab_available() {
                return None;
            }
            get_cached_or_prompt("GitLab", "GITLAB_TOKEN", &info.owner_repo)
        }
        PlatformKind::Bitbucket => {
            if let Some(tok) = crate::enrich::bitbucket::resolve_bitbucket_token() {
                return Some(tok);
            }
            get_cached_or_prompt("Bitbucket", "BITBUCKET_TOKEN", &info.owner_repo)
        }
        PlatformKind::Generic => None,
    }
}

fn get_cached_or_prompt(platform: &str, env_var: &str, repo_name: &str) -> Option<String> {
    use std::collections::HashMap;
    use std::sync::Mutex;

    static SESSION_CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

    let key = format!("{platform}:{repo_name}");
    if let Ok(mut guard) = SESSION_CACHE.lock() {
        let map = guard.get_or_insert_with(HashMap::new);
        if let Some(cached) = map.get(&key) {
            return cached.clone();
        }
        let token = prompt_interactive_token(platform, env_var, repo_name);
        map.insert(key, token.clone());
        token
    } else {
        prompt_interactive_token(platform, env_var, repo_name)
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

    let cli_cmd = match platform_name {
        "GitHub" => "gh auth login",
        "GitLab" => "glab auth login",
        _ => "",
    };

    eprintln!("💡 Tip: No {platform_name} authentication found for '{repo_name}'.");
    eprintln!("   To authenticate automatically in future runs, expose the environment variable:");
    eprintln!("     export {env_var}=\"<your-personal-access-token>\"");
    if !cli_cmd.is_empty() {
        eprintln!("   or authenticate via the official CLI:");
        eprintln!("     {cli_cmd}");
    }
    eprint!("   Enter {env_var} for this session (or press Enter to proceed offline): ");
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
                        association: None,
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
