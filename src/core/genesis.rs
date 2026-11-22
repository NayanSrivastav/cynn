use crate::core::accordion::AccordionSnippet;
use crate::core::Target;
use crate::enrich::github::PrDetails;
use crate::enrich::provider::resolve_remote_context;
use crate::enrich::test_match::find_related_tests;
use crate::git::{CommitInfo, GitRepo};
use serde::{Deserialize, Serialize};

/// Confidence classification based strictly on verifiable historical evidence depth
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Substantive PR description (>20 chars) OR substantive reviewer technical decision verified
    High,
    /// Substantive commit description (>20 chars) verified from Git history
    Medium,
    /// PR title or commit subject only (no extended rationale recorded)
    Low,
    /// No rationale found in commit or review history (never hallucinated)
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
    pub category: String, // "historical_rationale", "code_provenance", "companion_test"
    pub evidence_type: String, // "technical_rationale", "review_discussion", "pull_request_body", "commit_message", "test_suite"
    pub source: String,
    pub author: Option<String>,
    pub url: Option<String>,
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
    /// Indicates whether git line-history reconstruction successfully resolved the introducing commit.
    pub history_resolved: bool,
    /// Indicates whether line-history reconstruction fell back to the current HEAD blame commit.
    pub fallback_used: bool,
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
    /// Indicates whether git line-history reconstruction successfully resolved this historical milestone.
    pub history_resolved: bool,
    /// Indicates whether line-history reconstruction fell back to the current HEAD blame commit.
    pub fallback_used: bool,
}

/// Complete archaeological dossier for a target line or block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageDossier {
    pub file_path: String,
    pub line_range: String,
    pub code_snippet: String,
    pub is_multi_origin: bool,
    /// Introducing commit if the line range has a single origin.
    /// For multi-origin ranges, this is None: the individual `origins` slices are authoritative.
    pub genesis_commit: Option<CommitInfo>,
    pub origins: Vec<OriginSlice>,
    pub pr_reference: Option<String>,
    pub pr_details: Option<PrDetails>,
    pub intent_summary: String,
    pub confidence: Confidence,
    pub confidence_docs: String,
    /// Indicates whether line-history reconstruction fell back to unverified HEAD blame commits.
    pub fallback_used: bool,
    pub evidence: Vec<EvidenceItem>,
    pub related_tests: Vec<String>,
    pub evolution: Vec<EvolutionMilestone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet_accordion: Option<AccordionSnippet>,
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
        let is_multi_origin = origin_slices.len() > 1;

        // 4. Line History Reconstruction: Walk backward across revisions to find true genesis
        let (genesis_commit, milestones, provenance_fallback) =
            self.trace_provenance(&target.path, start_line, end_line, &origin_slices)?;

        let fallback_used = provenance_fallback || origin_slices.iter().any(|s| s.fallback_used);

        // 5. Context Enrichment & Origin Attribution
        let (final_genesis, intent_summary, confidence, pr_ref, pr_details) = if is_multi_origin {
            let summary = format!(
                "Multi-origin block ({} distinct introducing commits). Inspect individual origin slices for slice-level genesis and rationale.",
                origin_slices.len()
            );
            (None, summary, Confidence::Medium, None, None)
        } else {
            let remote_info = self.repo.get_remote_info();
            let mut pr_ref =
                Self::extract_pr_reference(&genesis_commit.subject, &genesis_commit.body);
            let mut pr_details =
                resolve_remote_context(&self.repo.root, remote_info.as_ref(), &genesis_commit);

            if pr_details.is_none() {
                for m in milestones.iter().rev() {
                    if let Ok(m_commit) = self.repo.get_commit(&m.commit_sha) {
                        if let Some(details) =
                            resolve_remote_context(&self.repo.root, remote_info.as_ref(), &m_commit)
                        {
                            if pr_ref.is_none() {
                                pr_ref = Some(format!("#{}", details.number));
                            }
                            pr_details = Some(details);
                            break;
                        }
                    }
                }
            }

            if let Some(ref pr) = pr_details {
                if pr_ref.is_none() {
                    pr_ref = Some(format!("#{}", pr.number));
                }
            }

            let (intent, conf) = Self::evaluate_intent_and_confidence(
                &genesis_commit,
                pr_details.as_ref(),
                fallback_used,
            );

            (
                Some(genesis_commit.clone()),
                intent,
                conf,
                pr_ref,
                pr_details,
            )
        };

        // 6. Companion Tests
        let related_tests = find_related_tests(&self.repo.root, &target.path);

        // 7. Assemble Structured Evidence Model
        let evidence = Self::assemble_evidence(
            final_genesis.as_ref(),
            &origin_slices,
            pr_details.as_ref(),
            &related_tests,
            fallback_used,
        );

        let snippet_accordion = match self.repo.read_file_lines(&target.path) {
            Ok(file_lines) => Some(AccordionSnippet::build(&file_lines, start_line, end_line)),
            Err(_) => None,
        };

        Ok(LineageDossier {
            file_path: target.path.display().to_string(),
            line_range: if start_line == end_line {
                format!("{start_line}")
            } else {
                format!("{start_line}-{end_line}")
            },
            code_snippet,
            is_multi_origin,
            genesis_commit: final_genesis,
            origins: origin_slices,
            pr_reference: pr_ref,
            pr_details,
            intent_summary,
            confidence,
            confidence_docs: "https://nayansrivastav.github.io/git-lineage/#confidence-tiers"
                .to_string(),
            fallback_used,
            evidence,
            related_tests,
            evolution: milestones,
            snippet_accordion,
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
                let slice =
                    self.resolve_slice_genesis(rel_path, group_start, group_end, &current_sha)?;
                slices.push(slice);

                group_start = entry.final_line;
                group_end = entry.final_line;
                current_sha.clone_from(&entry.sha);
            }
        }

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
        let (genesis_commit, history_resolved, fallback_used) =
            if let Ok(history) = self.repo.trace_line_evolution(rel_path, start, end, true) {
                if let Some(commit) = history.into_iter().last() {
                    (commit, true, false)
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
                    (commit, false, true)
                }
            } else {
                (self.repo.get_commit(fallback_sha)?, false, true)
            };

        Ok(OriginSlice {
            line_start: start,
            line_end: end,
            genesis_sha: genesis_commit.sha,
            short_sha: genesis_commit.short_sha,
            author: genesis_commit.author_name,
            date: genesis_commit.author_date,
            summary: genesis_commit.subject,
            history_resolved,
            fallback_used,
        })
    }

    /// Trace the full line history across revisions to reconstruct evolution milestones
    fn trace_provenance(
        &self,
        rel_path: &std::path::Path,
        start_line: usize,
        end_line: usize,
        origin_slices: &[OriginSlice],
    ) -> Result<(CommitInfo, Vec<EvolutionMilestone>, bool), String> {
        let is_multi_origin = origin_slices.len() > 1;
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
                    // For single-origin blocks, the oldest commit in the lineage is the sole genesis.
                    // For multi-origin blocks, no single commit in the aggregate timeline is the sole genesis;
                    // individual origins slices are authoritative.
                    let is_genesis = !is_multi_origin && (idx == total - 1);
                    let pr_ref = Self::extract_pr_reference(&commit.subject, &commit.body);

                    milestones.push(EvolutionMilestone {
                        date: commit.author_date,
                        commit_sha: commit.sha,
                        short_sha: commit.short_sha,
                        author: commit.author_name,
                        summary: commit.subject,
                        pr_reference: pr_ref,
                        is_genesis,
                        history_resolved: true,
                        fallback_used: false,
                    });
                }

                return Ok((genesis, milestones, false));
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
            is_genesis: !is_multi_origin,
            history_resolved: primary_slice.history_resolved,
            fallback_used: true,
        }];

        Ok((commit, milestones, true))
    }

    /// Strict confidence scoring based on verifiable evidence depth:
    /// High: Substantive PR description (>20 chars) OR substantive reviewer technical decision verified
    /// Medium: Substantive commit description (>20 chars)
    /// Low: PR title or commit subject only, or unverified DAG fallback
    /// None: No rationale recorded
    fn evaluate_intent_and_confidence(
        commit: &CommitInfo,
        pr_details: Option<&PrDetails>,
        fallback_used: bool,
    ) -> (String, Confidence) {
        if fallback_used {
            let (raw_intent, _) = Self::evaluate_intent_and_confidence_inner(commit, pr_details);
            return (
                format!("[Unverified DAG: fallback to HEAD blame commit] {raw_intent}"),
                Confidence::Low,
            );
        }
        Self::evaluate_intent_and_confidence_inner(commit, pr_details)
    }

    fn evaluate_intent_and_confidence_inner(
        commit: &CommitInfo,
        pr_details: Option<&PrDetails>,
    ) -> (String, Confidence) {
        if let Some(pr) = pr_details {
            // Case 1: Substantive PR description
            if let Some(ref summary) = pr.body_summary {
                if summary.trim().len() > 20 {
                    let full_intent =
                        if let Some(h) = pr.reviewer_highlights.iter().find(|h| h.is_technical) {
                            format!("{} — Technical Review ({}): {}", summary, h.author, h.body)
                        } else {
                            summary.clone()
                        };
                    return (format!("{}: {}", pr.title, full_intent), Confidence::High);
                }
            }

            // Case 2: Substantive technical reviewer rationale
            if let Some(h) = pr.reviewer_highlights.iter().find(|h| h.is_technical) {
                return (
                    format!("{}: Technical Review ({}): {}", pr.title, h.author, h.body),
                    Confidence::High,
                );
            }
        }

        // Case 3: Substantive commit body (>20 chars)
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

        // Case 4: PR title only (low confidence)
        if let Some(pr) = pr_details {
            return (format!("PR #{}: {}", pr.number, pr.title), Confidence::Low);
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

    /// Assemble structured evidence provenance items with clean attribution separation
    fn assemble_evidence(
        genesis_commit: Option<&CommitInfo>,
        origins: &[OriginSlice],
        pr_details: Option<&PrDetails>,
        related_tests: &[String],
        fallback_used: bool,
    ) -> Vec<EvidenceItem> {
        let mut evidence = Vec::new();

        if fallback_used {
            evidence.push(EvidenceItem {
                category: "code_provenance".to_string(),
                evidence_type: "unverified_fallback".to_string(),
                source: "git blame (fallback)".to_string(),
                author: None,
                url: None,
                excerpt: "Line evolution history could not be verified across revisions via log -L; fell back to HEAD blame commit.".to_string(),
            });
        }

        if let Some(pr) = pr_details {
            if let Some(ref body) = pr.body_summary {
                evidence.push(EvidenceItem {
                    category: "historical_rationale".to_string(),
                    evidence_type: "pull_request_body".to_string(),
                    source: format!("PR #{}", pr.number),
                    author: pr.author.clone(),
                    url: Some(pr.url.clone()),
                    excerpt: body.clone(),
                });
            }
            for highlight in &pr.reviewer_highlights {
                evidence.push(EvidenceItem {
                    category: "historical_rationale".to_string(),
                    evidence_type: if highlight.is_technical {
                        "technical_rationale".to_string()
                    } else {
                        "review_discussion".to_string()
                    },
                    source: format!("PR #{}", pr.number),
                    author: Some(highlight.author.clone()),
                    url: highlight.url.clone(),
                    excerpt: highlight.body.clone(),
                });
            }
        }

        if let Some(commit) = genesis_commit {
            if !commit.body.is_empty() {
                evidence.push(EvidenceItem {
                    category: "code_provenance".to_string(),
                    evidence_type: "commit_message".to_string(),
                    source: format!("commit {}", commit.short_sha),
                    author: Some(commit.author_name.clone()),
                    url: None,
                    excerpt: commit.body.clone(),
                });
            }
        } else {
            // For multi-origin blocks, include each slice introducing commit
            for slice in origins {
                evidence.push(EvidenceItem {
                    category: "code_provenance".to_string(),
                    evidence_type: "slice_origin".to_string(),
                    source: format!("lines {}-{}", slice.line_start, slice.line_end),
                    author: Some(slice.author.clone()),
                    url: None,
                    excerpt: format!("commit {}: {}", slice.short_sha, slice.summary),
                });
            }
        }

        for test in related_tests {
            evidence.push(EvidenceItem {
                category: "companion_test".to_string(),
                evidence_type: "test_suite".to_string(),
                source: test.clone(),
                author: None,
                url: None,
                excerpt: format!("Candidate companion test file {test}"),
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
