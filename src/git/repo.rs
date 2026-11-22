use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Commit metadata extracted from git history
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub author_name: String,
    pub author_email: String,
    pub author_date: String,
    pub subject: String,
    pub body: String,
}

/// A line blame entry from git blame --line-porcelain
#[derive(Debug, Clone)]
pub struct BlameEntry {
    pub sha: String,
    pub original_line: usize,
    pub final_line: usize,
    pub author: String,
    pub author_mail: String,
    pub author_time: i64,
    pub summary: String,
    pub code: String,
}

/// Remote platform classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformKind {
    GitHub,
    GitLab,
    Bitbucket,
    Generic,
}

impl std::fmt::Display for PlatformKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitHub => write!(f, "GitHub"),
            Self::GitLab => write!(f, "GitLab"),
            Self::Bitbucket => write!(f, "Bitbucket"),
            Self::Generic => write!(f, "Generic Git"),
        }
    }
}

/// Structured metadata about a remote origin repository
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub platform: PlatformKind,
    pub host: String,
    pub owner_repo: String,
    pub raw_url: String,
}

pub struct GitRepo {
    pub root: PathBuf,
}

impl GitRepo {
    /// Discover the git repository root containing the given path
    pub fn discover(start_path: &Path) -> Result<Self, String> {
        let output = Command::new("git")
            .arg("rev-parse")
            .arg("--show-toplevel")
            .current_dir(if start_path.is_dir() {
                start_path
            } else {
                start_path.parent().unwrap_or(Path::new("."))
            })
            .output()
            .map_err(|e| format!("Failed to execute git: {e}"))?;

        if !output.status.success() {
            return Err("Not inside a Git repository".to_string());
        }

        let root_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(Self {
            root: PathBuf::from(root_str),
        })
    }

    /// Extract structured remote repository information from the origin URL
    pub fn get_remote_info(&self) -> Option<RemoteInfo> {
        let output = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&self.root)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Self::parse_remote_url(&raw)
    }

    /// Extract the remote owner/repo identifier if origin is configured
    pub fn get_remote_repo(&self) -> Option<String> {
        self.get_remote_info().map(|r| r.owner_repo)
    }

    /// Parse a remote Git URL into structured platform, host, and owner/repo metadata
    #[allow(clippy::too_many_lines)]
    pub fn parse_remote_url(raw: &str) -> Option<RemoteInfo> {
        if raw.is_empty() {
            return None;
        }

        let mut cleaned = raw.trim();
        if let Some(idx) = cleaned.find('?') {
            cleaned = &cleaned[..idx];
        }
        if let Some(idx) = cleaned.find('#') {
            cleaned = &cleaned[..idx];
        }
        let trimmed = cleaned.trim_end_matches(".git");

        let (platform, host, owner_repo) =
            if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
                (
                    PlatformKind::GitHub,
                    "github.com".to_string(),
                    clean_owner_repo(PlatformKind::GitHub, rest),
                )
            } else if let Some(rest) = trimmed.strip_prefix("https://github.com/") {
                (
                    PlatformKind::GitHub,
                    "github.com".to_string(),
                    clean_owner_repo(PlatformKind::GitHub, rest),
                )
            } else if let Some(rest) = trimmed.strip_prefix("ssh://git@github.com/") {
                (
                    PlatformKind::GitHub,
                    "github.com".to_string(),
                    clean_owner_repo(PlatformKind::GitHub, rest),
                )
            } else if let Some(rest) = trimmed.strip_prefix("git@gitlab.com:") {
                (
                    PlatformKind::GitLab,
                    "gitlab.com".to_string(),
                    clean_owner_repo(PlatformKind::GitLab, rest),
                )
            } else if let Some(rest) = trimmed.strip_prefix("https://gitlab.com/") {
                (
                    PlatformKind::GitLab,
                    "gitlab.com".to_string(),
                    clean_owner_repo(PlatformKind::GitLab, rest),
                )
            } else if let Some(rest) = trimmed.strip_prefix("ssh://git@gitlab.com/") {
                (
                    PlatformKind::GitLab,
                    "gitlab.com".to_string(),
                    clean_owner_repo(PlatformKind::GitLab, rest),
                )
            } else if let Some(rest) = trimmed.strip_prefix("git@bitbucket.org:") {
                (
                    PlatformKind::Bitbucket,
                    "bitbucket.org".to_string(),
                    clean_owner_repo(PlatformKind::Bitbucket, rest),
                )
            } else if let Some(rest) = trimmed.strip_prefix("https://bitbucket.org/") {
                (
                    PlatformKind::Bitbucket,
                    "bitbucket.org".to_string(),
                    clean_owner_repo(PlatformKind::Bitbucket, rest),
                )
            } else if let Some(idx) = trimmed.find("bitbucket.org/") {
                let rest = &trimmed[idx + "bitbucket.org/".len()..];
                (
                    PlatformKind::Bitbucket,
                    "bitbucket.org".to_string(),
                    clean_owner_repo(PlatformKind::Bitbucket, rest),
                )
            } else if trimmed.contains("gitlab") {
                let parts: Vec<&str> = trimmed
                    .split(['/', ':'])
                    .filter(|s| {
                        !s.is_empty() && *s != "https" && *s != "http" && *s != "ssh" && *s != "git"
                    })
                    .collect();
                if parts.len() >= 3 {
                    let host = parts[0].to_string();
                    let owner_repo = parts[1..].join("/");
                    (
                        PlatformKind::GitLab,
                        host,
                        clean_owner_repo(PlatformKind::GitLab, &owner_repo),
                    )
                } else {
                    (
                        PlatformKind::Generic,
                        "unknown".to_string(),
                        trimmed.to_string(),
                    )
                }
            } else {
                (
                    PlatformKind::Generic,
                    "generic".to_string(),
                    trimmed.to_string(),
                )
            };

        Some(RemoteInfo {
            platform,
            host,
            owner_repo: owner_repo.trim_matches('/').to_string(),
            raw_url: raw.trim().to_string(),
        })
    }

    /// Run git blame on a specific line or range with whitespace/noise filtering
    pub fn blame_lines(
        &self,
        rel_path: &Path,
        start_line: usize,
        end_line: usize,
        ignore_whitespace: bool,
    ) -> Result<Vec<BlameEntry>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("blame")
            .arg("--line-porcelain")
            .arg(format!("-L{start_line},{end_line}"));

        if ignore_whitespace {
            cmd.arg("-w");
        }

        cmd.arg("--").arg(rel_path).current_dir(&self.root);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git blame: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git blame failed: {stderr}"));
        }

        Ok(Self::parse_porcelain_blame(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    /// Read lines of a file from the repository worktree, falling back to git show HEAD:<path>
    pub fn read_file_lines(&self, rel_path: &Path) -> Result<Vec<String>, String> {
        let abs_path = self.root.join(rel_path);
        if abs_path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&abs_path) {
                return Ok(content.lines().map(String::from).collect());
            }
        }

        // Cross-platform normalization: git expects forward slashes
        let git_path = rel_path.to_string_lossy().replace('\\', "/");
        let output = Command::new("git")
            .arg("show")
            .arg(format!("HEAD:{git_path}"))
            .current_dir(&self.root)
            .output()
            .map_err(|e| format!("Failed to read file from git: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Could not read file '{git_path}': {stderr}"));
        }

        let content = String::from_utf8_lossy(&output.stdout);
        Ok(content.lines().map(String::from).collect())
    }

    /// Retrieve full commit metadata for a specific SHA
    pub fn get_commit(&self, sha: &str) -> Result<CommitInfo, String> {
        // Format: %H%x00%h%x00%an%x00%ae%x00%ad%x00%s%x00%b
        let output = Command::new("git")
            .arg("show")
            .arg("-s")
            .arg("--format=%H%x00%h%x00%an%x00%ae%x00%ad%x00%s%x00%b")
            .arg(sha)
            .current_dir(&self.root)
            .output()
            .map_err(|e| format!("Failed to execute git show: {e}"))?;

        if !output.status.success() {
            return Err(format!("Commit '{sha}' not found"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.split('\0').collect();

        if parts.len() < 7 {
            return Err(format!("Unexpected git show output format for '{sha}'"));
        }

        Ok(CommitInfo {
            sha: parts[0].to_string(),
            short_sha: parts[1].to_string(),
            author_name: parts[2].to_string(),
            author_email: parts[3].to_string(),
            author_date: parts[4].to_string(),
            subject: parts[5].to_string(),
            body: parts[6].trim().to_string(),
        })
    }

    /// Trace the evolutionary history of a line range backward through the Git DAG.
    /// Returns the commits that touched this range, ordered from most recent to oldest.
    /// The last commit in this list is the True Genesis Commit where the lines were born.
    pub fn trace_line_evolution(
        &self,
        rel_path: &Path,
        start_line: usize,
        end_line: usize,
        ignore_whitespace: bool,
    ) -> Result<Vec<CommitInfo>, String> {
        let mut cmd = Command::new("git");
        cmd.arg("log")
            .arg(format!("-L{start_line},{end_line}:{}", rel_path.display()))
            .arg("-C")
            .arg("--no-patch")
            .arg("--format=%H%x00%h%x00%an%x00%ae%x00%ad%x00%s%x00%b%x1f");

        if ignore_whitespace {
            cmd.arg("-w");
        }

        cmd.current_dir(&self.root);

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to run git log -L: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("git log -L failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut commits = Vec::new();

        for chunk in stdout.split('\x1f') {
            let trimmed = chunk.trim();
            if trimmed.is_empty() {
                continue;
            }
            let parts: Vec<&str> = trimmed.split('\0').collect();
            if parts.len() >= 7 {
                commits.push(CommitInfo {
                    sha: parts[0].to_string(),
                    short_sha: parts[1].to_string(),
                    author_name: parts[2].to_string(),
                    author_email: parts[3].to_string(),
                    author_date: parts[4].to_string(),
                    subject: parts[5].to_string(),
                    body: parts[6].trim().to_string(),
                });
            }
        }

        Ok(commits)
    }

    /// Parse output of git blame --line-porcelain
    fn parse_porcelain_blame(content: &str) -> Vec<BlameEntry> {
        let mut entries = Vec::new();
        let mut current_sha = String::new();
        let mut orig_line = 0;
        let mut final_line = 0;
        let mut author = String::new();
        let mut author_mail = String::new();
        let mut author_time = 0;
        let mut summary = String::new();

        for line in content.lines() {
            if let Some(rest) = line.strip_prefix('\t') {
                // The code line itself finishes a porcelain block
                entries.push(BlameEntry {
                    sha: current_sha.clone(),
                    original_line: orig_line,
                    final_line,
                    author: author.clone(),
                    author_mail: author_mail.clone(),
                    author_time,
                    summary: summary.clone(),
                    code: rest.to_string(),
                });
            } else if line.len() >= 40 && line.chars().take(40).all(|c| c.is_ascii_hexdigit()) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    current_sha = parts[0].to_string();
                    orig_line = parts[1].parse().unwrap_or(0);
                    final_line = parts[2].parse().unwrap_or(0);
                }
            } else if let Some(val) = line.strip_prefix("author ") {
                author = val.to_string();
            } else if let Some(val) = line.strip_prefix("author-mail ") {
                author_mail = val.trim_matches(|c| c == '<' || c == '>').to_string();
            } else if let Some(val) = line.strip_prefix("author-time ") {
                author_time = val.parse().unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("summary ") {
                summary = val.to_string();
            }
        }

        entries
    }
}

fn clean_owner_repo(platform: PlatformKind, path: &str) -> String {
    let without_git = path.trim_end_matches(".git").trim_matches('/');
    match platform {
        PlatformKind::GitHub => {
            let s = without_git.split("/blob/").next().unwrap_or(without_git);
            s.split("/tree/")
                .next()
                .unwrap_or(s)
                .trim_matches('/')
                .to_string()
        }
        PlatformKind::GitLab => without_git
            .split("/-/")
            .next()
            .unwrap_or(without_git)
            .trim_matches('/')
            .to_string(),
        PlatformKind::Bitbucket => {
            let s = without_git.split("/src/").next().unwrap_or(without_git);
            let s = s.split("/commits/").next().unwrap_or(s);
            s.split("/pull-requests/")
                .next()
                .unwrap_or(s)
                .trim_matches('/')
                .to_string()
        }
        PlatformKind::Generic => without_git.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_remote_url_github() {
        let ssh =
            GitRepo::parse_remote_url("git@github.com:NayanSrivastav/git-lineage.git").unwrap();
        assert_eq!(ssh.platform, PlatformKind::GitHub);
        assert_eq!(ssh.host, "github.com");
        assert_eq!(ssh.owner_repo, "NayanSrivastav/git-lineage");

        let https =
            GitRepo::parse_remote_url("https://github.com/NayanSrivastav/git-lineage").unwrap();
        assert_eq!(https.platform, PlatformKind::GitHub);
        assert_eq!(https.owner_repo, "NayanSrivastav/git-lineage");
    }

    #[test]
    fn test_parse_remote_url_gitlab() {
        let web = GitRepo::parse_remote_url("https://gitlab.com/kbutu-group/postgre").unwrap();
        assert_eq!(web.platform, PlatformKind::GitLab);
        assert_eq!(web.host, "gitlab.com");
        assert_eq!(web.owner_repo, "kbutu-group/postgre");

        let web_file = GitRepo::parse_remote_url(
            "https://gitlab.com/kbutu-group/postgre/-/blob/main/README.md",
        )
        .unwrap();
        assert_eq!(web_file.platform, PlatformKind::GitLab);
        assert_eq!(web_file.owner_repo, "kbutu-group/postgre");

        let ssh = GitRepo::parse_remote_url("git@gitlab.com:kbutu-group/postgre.git").unwrap();
        assert_eq!(ssh.platform, PlatformKind::GitLab);
        assert_eq!(ssh.owner_repo, "kbutu-group/postgre");
    }

    #[test]
    fn test_parse_remote_url_bitbucket() {
        let web_with_query = GitRepo::parse_remote_url(
            "https://bitbucket.org/fargo3d/public/src/2f230c6334a85fbee3818bdf9ad9b3367c9f778b/?at=release%2Fpublic",
        )
        .unwrap();
        assert_eq!(web_with_query.platform, PlatformKind::Bitbucket);
        assert_eq!(web_with_query.host, "bitbucket.org");
        assert_eq!(web_with_query.owner_repo, "fargo3d/public");

        let ssh = GitRepo::parse_remote_url("git@bitbucket.org:fargo3d/public.git").unwrap();
        assert_eq!(ssh.platform, PlatformKind::Bitbucket);
        assert_eq!(ssh.owner_repo, "fargo3d/public");
    }
}
