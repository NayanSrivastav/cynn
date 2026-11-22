use crate::core::target::Target;
use crate::enrich::{find_related_tests, resolve_pr_for_commit, PrDetails};
use crate::git::{CommitInfo, GitRepo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Confidence {
    #[serde(rename = "high")]
    High,
    #[serde(rename = "medium")]
    Medium,
    #[serde(rename = "low")]
    Low,
    #[serde(rename = "none")]
    None,
}

impl std::fmt::Display for Confidence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::High => write!(
                f,
                "High (Substantive PR rationale or reviewer decision verified)"
            ),
            Self::Medium => write!(f, "Medium (Inferred from commit description)"),
            Self::Low => write!(f, "Low (PR title or commit subject only)"),
            Self::None => write!(f, "None (No explicit rationale recorded in history)"),
        }
    }
}

/// Structured evidence piece supporting code provenance and intent claims
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub evidence_type: String,
    pub source: String,
    pub author: Option<String>,
    pub excerpt: String,
}

/// A slice of lines within a range that originated from a specific commit
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OriginSlice {
    pub line_start: usize,
    pub line_end: usize,
    pub genesis_sha: String,
    pub short_sha: String,
    pub author: String,
    pub date: String,
    pub summary: String,
    pub is_verified: bool,
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
    pub is_verified: bool,
}

/// Complete archaeological dossier for a target line or block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageDossier {
    pub file_path: String,
    pub line_range: String,
    pub code_snippet: String,
    pub genesis_commit: CommitInfo,
    pub origins: Vec<OriginSlice>,
    pub pr_reference: Option<String>,
    pub pr_details: Option<PrDetails>,
    pub intent_summary: String,
    pub confidence: Confidence,
    pub evidence: Vec<EvidenceItem>,
    pub related_tests: Vec<String>,
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

        // 1. Initial porcelain blame with whitespace filtering (-w)
        let entries = self
            .repo
            .blame_lines(&target.path, start_line, end_line, true)?;

        if entries.is_empty() {
            return Err("No blame information available for specified lines".to_string());
        }

        // 2. Extract code snippet
        let code_snippet = entries
            .iter()
            .map(|e| e.code.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // 3. Multi-Origin Dissection: Group contiguous lines by their HEAD commit SHA
        let origin_slices = self.dissect_origins(&target.path, &entries)?;

        // 4. Line History Reconstruction: Walk backward across revisions to find true genesis
        let (genesis_commit, milestones) =
            self.trace_provenance(&target.path, start_line, end_line, &origin_slices)?;

        // 5. Context Enrichment: PR details, discussions, and linked issues with explicit repo context
        let remote_repo = self.repo.get_remote_repo();
        let mut pr_ref = Self::extract_pr_reference(&genesis_commit.subject, &genesis_commit.body);
        let mut pr_details =
            resolve_pr_for_commit(&self.repo.root, remote_repo.as_deref(), &genesis_commit.sha);

        // If no PR was found on genesis, check if any milestone commit has a linked PR
        if pr_details.is_none() {
            for m in milestones.iter().rev() {
                if let Some(details) =
                    resolve_pr_for_commit(&self.repo.root, remote_repo.as_deref(), &m.commit_sha)
                {
                    if pr_ref.is_none() {
                        pr_ref = Some(format!("#{}", details.number));
                    }
                    pr_details = Some(details);
                    break;
                }
            }
        }

        if let Some(ref pr) = pr_details {
            if pr_ref.is_none() {
                pr_ref = Some(format!("#{}", pr.number));
            }
        }

        // 6. Strict Intent & Confidence Engine (Facts vs Interpretations)
        let (intent_summary, confidence) =
            Self::evaluate_intent_and_confidence(&genesis_commit, pr_details.as_ref());

        // 7. Companion Tests
        let related_tests = find_related_tests(&self.repo.root, &target.path);

        // 8. Assemble Structured Evidence Model
        let evidence =
            Self::assemble_evidence(&genesis_commit, pr_details.as_ref(), &related_tests);

        Ok(LineageDossier {
            file_path: target.path.display().to_string(),
            line_range: if start_line == end_line {
                format!("{start_line}")
            } else {
                format!("{start_line}-{end_line}")
            },
            code_snippet,
            genesis_commit,
            origins: origin_slices,
            pr_reference: pr_ref,
            pr_details,
            intent_summary,
            confidence,
            evidence,
            related_tests,
            evolution: milestones,
        })
    }

    /// Dissect a line range into contiguous slices and trace each slice to its introducing commit
    fn dissect_origins(
        &self,
        rel_path: &std::path::Path,
        entries: &[crate::git::BlameEntry],
    ) -> Result<Vec<OriginSlice>, String> {
        let mut slices = Vec::new();
        if entries.is_empty() {
            return Ok(slices);
        }

        let mut group_start = entries[0].final_line;
        let mut group_end = entries[0].final_line;
        let mut current_sha = entries[0].sha.clone();

        for entry in entries.iter().skip(1) {
            if entry.sha == current_sha {
                group_end = entry.final_line;
            } else {
                // Finish current slice
                let slice =
                    self.resolve_slice_genesis(rel_path, group_start, group_end, &current_sha)?;
                slices.push(slice);

                group_start = entry.final_line;
                group_end = entry.final_line;
                current_sha.clone_from(&entry.sha);
            }
        }

        // Add final slice
        let slice = self.resolve_slice_genesis(rel_path, group_start, group_end, &current_sha)?;
        slices.push(slice);

        Ok(slices)
    }

    fn resolve_slice_genesis(
        &self,
        rel_path: &std::path::Path,
        start: usize,
        end: usize,
        fallback_sha: &str,
    ) -> Result<OriginSlice, String> {
        // Attempt line evolution trace to locate introducing commit
        let (genesis_commit, is_verified) =
            if let Ok(history) = self.repo.trace_line_evolution(rel_path, start, end, true) {
                if let Some(commit) = history.into_iter().last() {
                    (commit, true)
                } else {
                    let commit =
                        self.repo
                            .get_commit(fallback_sha)
                            .unwrap_or_else(|_| CommitInfo {
                                sha: fallback_sha.to_string(),
                                short_sha: fallback_sha.chars().take(7).collect(),
                                author_name: "Unknown".to_string(),
                                author_email: String::new(),
                                author_date: String::new(),
                                subject: String::new(),
                                body: String::new(),
                            });
                    (commit, false)
                }
            } else {
                (self.repo.get_commit(fallback_sha)?, false)
            };

        Ok(OriginSlice {
            line_start: start,
            line_end: end,
            genesis_sha: genesis_commit.sha,
            short_sha: genesis_commit.short_sha,
            author: genesis_commit.author_name,
            date: genesis_commit.author_date,
            summary: genesis_commit.subject,
            is_verified,
        })
    }

    /// Trace the full line history across revisions to reconstruct evolution milestones
    fn trace_provenance(
        &self,
        rel_path: &std::path::Path,
        start_line: usize,
        end_line: usize,
        origin_slices: &[OriginSlice],
    ) -> Result<(CommitInfo, Vec<EvolutionMilestone>), String> {
        let commits_history = self
            .repo
            .trace_line_evolution(rel_path, start_line, end_line, true)
            .ok();

        if let Some(history) = commits_history {
            if !history.is_empty() {
                let total = history.len();
                let genesis = history.last().unwrap().clone();

                let mut milestones = Vec::new();
                for (idx, commit) in history.into_iter().enumerate() {
                    let is_genesis = idx == total - 1;
                    let pr_ref = Self::extract_pr_reference(&commit.subject, &commit.body);

                    milestones.push(EvolutionMilestone {
                        date: commit.author_date,
                        commit_sha: commit.sha,
                        short_sha: commit.short_sha,
                        author: commit.author_name,
                        summary: commit.subject,
                        pr_reference: pr_ref,
                        is_genesis,
                        is_verified: true,
                    });
                }

                return Ok((genesis, milestones));
            }
        }

        // Fallback to primary origin slice if log -L is unavailable
        let primary_slice = &origin_slices[0];
        let commit = self.repo.get_commit(&primary_slice.genesis_sha)?;
        let pr_ref = Self::extract_pr_reference(&commit.subject, &commit.body);

        let milestones = vec![EvolutionMilestone {
            date: commit.author_date.clone(),
            commit_sha: commit.sha.clone(),
            short_sha: commit.short_sha.clone(),
            author: commit.author_name.clone(),
            summary: commit.subject.clone(),
            pr_reference: pr_ref,
            is_genesis: true,
            is_verified: primary_slice.is_verified,
        }];

        Ok((commit, milestones))
    }

    /// Strict confidence scoring based on verifiable evidence depth:
    /// High: Substantive PR description (>20 chars) OR reviewer explanation
    /// Medium: Substantive commit description (>20 chars)
    /// Low: PR title or commit subject only
    /// None: No rationale recorded
    fn evaluate_intent_and_confidence(
        commit: &CommitInfo,
        pr_details: Option<&PrDetails>,
    ) -> (String, Confidence) {
        if let Some(pr) = pr_details {
            // Case 1: Substantive PR description
            if let Some(ref summary) = pr.body_summary {
                if summary.trim().len() > 20 {
                    let full_intent = if pr.reviewer_highlights.is_empty() {
                        summary.clone()
                    } else {
                        format!("{} — Review Note: {}", summary, pr.reviewer_highlights[0])
                    };
                    return (format!("{}: {}", pr.title, full_intent), Confidence::High);
                }
            }

            // Case 2: Substantive reviewer discussion notes
            if !pr.reviewer_highlights.is_empty() {
                return (
                    format!("{}: Review Note: {}", pr.title, pr.reviewer_highlights[0]),
                    Confidence::High,
                );
            }

            // Case 3: PR exists but only has a title (low confidence)
            return (format!("PR #{}: {}", pr.number, pr.title), Confidence::Low);
        }

        // Case 4: Substantive commit body (>20 chars)
        if !commit.body.is_empty() {
            let first_para = commit
                .body
                .split("\n\n")
                .next()
                .unwrap_or(&commit.body)
                .replace('\n', " ");

            let trimmed = first_para.trim();
            if trimmed.len() > 20 {
                return (trimmed.to_string(), Confidence::Medium);
            } else if !trimmed.is_empty() {
                return (trimmed.to_string(), Confidence::Low);
            }
        }

        // Case 5: Commit subject only
        if !commit.subject.is_empty() {
            return (commit.subject.clone(), Confidence::Low);
        }

        // Case 6: No rationale recorded
        (
            "[No explicit rationale recorded in commit or PR history]".to_string(),
            Confidence::None,
        )
    }

    /// Assemble structured evidence provenance items
    fn assemble_evidence(
        commit: &CommitInfo,
        pr_details: Option<&PrDetails>,
        related_tests: &[String],
    ) -> Vec<EvidenceItem> {
        let mut evidence = Vec::new();

        if let Some(pr) = pr_details {
            if let Some(ref body) = pr.body_summary {
                evidence.push(EvidenceItem {
                    evidence_type: "pull_request_body".to_string(),
                    source: format!("PR #{}", pr.number),
                    author: None,
                    excerpt: body.clone(),
                });
            }
            for highlight in &pr.reviewer_highlights {
                evidence.push(EvidenceItem {
                    evidence_type: "review_discussion".to_string(),
                    source: format!("PR #{}", pr.number),
                    author: None,
                    excerpt: highlight.clone(),
                });
            }
        }

        if !commit.body.is_empty() {
            evidence.push(EvidenceItem {
                evidence_type: "commit_message".to_string(),
                source: format!("commit {}", commit.short_sha),
                author: Some(commit.author_name.clone()),
                excerpt: commit.body.clone(),
            });
        }

        for test in related_tests {
            evidence.push(EvidenceItem {
                evidence_type: "companion_test".to_string(),
                source: test.clone(),
                author: None,
                excerpt: "companion test suite".to_string(),
            });
        }

        evidence
    }

    /// Extract PR reference number if present in commit messages
    fn extract_pr_reference(subject: &str, body: &str) -> Option<String> {
        let full_text = format!("{subject}\n{body}");
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
}
