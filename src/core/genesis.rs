use crate::core::target::Target;
use crate::git::{CommitInfo, GitRepo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Confidence {
    #[serde(rename = "explicit")]
    Explicit,
    #[serde(rename = "inferred")]
    Inferred,
    #[serde(rename = "none")]
    None,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit => write!(f, "High (Explicit PR / issue rationale found)"),
            Self::Inferred => write!(f, "Medium (Inferred from commit description)"),
            Self::None => write!(f, "Low (No rationale recorded in commit message)"),
        }
    }
}

/// Evolution milestone representing a historical touchpoint for the code
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionMilestone {
    pub date: String,
    pub commit_sha: String,
    pub short_sha: String,
    pub author: String,
    pub summary: String,
    pub pr_reference: Option<String>,
    pub is_genesis: bool,
}

/// Complete archaeological dossier for a target line or block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageDossier {
    pub file_path: String,
    pub line_range: String,
    pub code_snippet: String,
    pub genesis_commit: CommitInfo,
    pub pr_reference: Option<String>,
    pub intent_summary: String,
    pub confidence: Confidence,
    pub evolution: Vec<EvolutionMilestone>,
}

pub struct GenesisAnalyzer<'a> {
    repo: &'a GitRepo,
}

impl<'a> GenesisAnalyzer<'a> {
    pub fn new(repo: &'a GitRepo) -> Self {
        Self { repo }
    }

    /// Analyze the target file and line range to assemble the archaeological dossier
    pub fn analyze(&self, target: &Target) -> Result<LineageDossier, String> {
        let start_line = target.start_line.unwrap_or(1);
        let end_line = target.end_line.unwrap_or(start_line);

        // Blame lines with whitespace filtering (-w)
        let entries = self
            .repo
            .blame_lines(&target.path, start_line, end_line, true)?;

        if entries.is_empty() {
            return Err("No blame information available for specified lines".to_string());
        }

        // Identify the primary introducing commit among the lines
        let primary_sha = &entries[0].sha;
        let commit = self.repo.get_commit(primary_sha)?;

        // Extract code snippet
        let code_snippet = entries
            .iter()
            .map(|e| e.code.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // Parse PR references from subject or body (e.g. #123 or Merge pull request #123)
        let pr_ref = Self::extract_pr_reference(&commit.subject, &commit.body);

        // Determine intent summary and confidence
        let (intent_summary, confidence) = Self::extract_intent(&commit, pr_ref.is_some());

        // Construct milestones
        let milestones = vec![EvolutionMilestone {
            date: commit.author_date.clone(),
            commit_sha: commit.sha.clone(),
            short_sha: commit.short_sha.clone(),
            author: commit.author_name.clone(),
            summary: commit.subject.clone(),
            pr_reference: pr_ref.clone(),
            is_genesis: true,
        }];

        Ok(LineageDossier {
            file_path: target.path.display().to_string(),
            line_range: if start_line == end_line {
                format!("{start_line}")
            } else {
                format!("{start_line}-{end_line}")
            },
            code_snippet,
            genesis_commit: commit,
            pr_reference: pr_ref,
            intent_summary,
            confidence,
            evolution: milestones,
        })
    }

    /// Extract PR reference number if present in commit messages
    fn extract_pr_reference(subject: &str, body: &str) -> Option<String> {
        let full_text = format!("{subject}\n{body}");

        // Look for common patterns: (#123), Merge pull request #123, PR #123
        for word in full_text.split_whitespace() {
            let clean = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '#');
            if let Some(num_str) = clean.strip_prefix('#') {
                if num_str.chars().all(|c| c.is_ascii_digit()) && !num_str.is_empty() {
                    return Some(format!("#{num_str}"));
                }
            }
        }

        None
    }

    /// Extract intent summary and determine truthfulness confidence
    fn extract_intent(commit: &CommitInfo, has_pr: bool) -> (String, Confidence) {
        if !commit.body.is_empty() {
            let first_para = commit
                .body
                .split("\n\n")
                .next()
                .unwrap_or(&commit.body)
                .replace('\n', " ");

            let confidence = if has_pr {
                Confidence::Explicit
            } else {
                Confidence::Inferred
            };

            (first_para, confidence)
        } else if !commit.subject.is_empty() {
            (
                commit.subject.clone(),
                if has_pr {
                    Confidence::Explicit
                } else {
                    Confidence::Inferred
                },
            )
        } else {
            (
                "[No explicit rationale recorded in commit message]".to_string(),
                Confidence::None,
            )
        }
    }
}
