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

    /// Extract the remote GitHub owner/repo identifier if origin is configured
    pub fn get_remote_repo(&self) -> Option<String> {
        let output = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&self.root)
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Handle git@github.com:owner/repo.git or https://github.com/owner/repo.git
        let cleaned = raw
            .strip_prefix("git@github.com:")
            .or_else(|| raw.strip_prefix("https://github.com/"))?;

        let repo = cleaned.trim_end_matches(".git").trim();
        if repo.is_empty() {
            None
        } else {
            Some(repo.to_string())
        }
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
