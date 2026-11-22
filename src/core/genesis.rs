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
    pub blame_sha: String,
    pub blame_short_sha: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blame_commits: Vec<String>,
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
    /// Status of remote PR/issue enrichment (`"enriched"`, `"trailer_resolved"`, `"no_matching_pr"`, `"offline"`)
    pub enrichment_status: String,
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
    #[allow(clippy::too_many_lines)]
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

        // Determine multi-origin status based on DISTINCT resolved introducing commits (genesis_sha),
        // not merely the count of contiguous blame slices. Multiple slices originating from the
        // same introducing commit belong to a single logical origin.
        let mut distinct_genesis_shas: Vec<&str> = origin_slices
            .iter()
            .map(|s| s.genesis_sha.as_str())
            .collect();
        distinct_genesis_shas.sort_unstable();
        distinct_genesis_shas.dedup();
        let is_multi_origin = distinct_genesis_shas.len() > 1;

        // 4. Line History Reconstruction: Walk backward across revisions to find true genesis
        let (genesis_commit, milestones, provenance_fallback) =
            self.trace_provenance(&target.path, start_line, end_line, &origin_slices)?;

        let fallback_used = provenance_fallback || origin_slices.iter().any(|s| s.fallback_used);

        // 5. Context Enrichment & Origin Attribution
        let (final_genesis, intent_summary, confidence, pr_ref, pr_details) = if is_multi_origin {
            let any_substantive = origin_slices.iter().any(|s| {
                if let Ok(c) = self.repo.get_commit(&s.genesis_sha) {
                    Self::extract_substantive_commit_body(&c.body).is_some()
                } else {
                    false
                }
            });

            let conf = if fallback_used {
                Confidence::Low
            } else if any_substantive {
                Confidence::Medium
            } else {
                Confidence::Low
            };

            let fallback_note = if fallback_used {
                "; unverified fallback used for one or more slices"
            } else {
                ""
            };
            let distinct_count = distinct_genesis_shas.len();
            let commit_str = if distinct_count == 1 {
                "1 distinct introducing commit".to_string()
            } else {
                format!("{distinct_count} distinct introducing commits")
            };
            let slice_str = if origin_slices.len() == 1 {
                "1 slice".to_string()
            } else {
                format!("{} slices", origin_slices.len())
            };
            let summary = format!(
                "Multi-origin block ({commit_str} across {slice_str}{fallback_note}). Inspect individual origin slices for slice-level genesis and rationale."
            );
            (None, summary, conf, None, None)
        } else {
            let remote_info = self.repo.get_remote_info();
            // If all slices share a single genesis_sha, resolve that authoritative commit
            let single_genesis_commit = if origin_slices.len() > 1 && !is_multi_origin {
                self.repo
                    .get_commit(distinct_genesis_shas[0])
                    .unwrap_or(genesis_commit)
            } else {
                genesis_commit
            };

            let mut pr_ref = Self::extract_pr_reference(
                &single_genesis_commit.subject,
                &single_genesis_commit.body,
            );
            let mut pr_details = resolve_remote_context(
                &self.repo.root,
                remote_info.as_ref(),
                &single_genesis_commit,
            );

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
                &single_genesis_commit,
                pr_details.as_ref(),
                fallback_used,
            );

            (
                Some(single_genesis_commit),
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

        let enrichment_status = if pr_details.is_some() {
            "enriched".to_string()
        } else if pr_ref.is_some() {
            "trailer_resolved".to_string()
        } else if self.repo.get_remote_info().is_some() {
            "no_matching_pr".to_string()
        } else {
            "offline".to_string()
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
            enrichment_status,
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
        if entries.is_empty() {
            return Ok(Vec::new());
        }

        // 1. Group contiguous blame entries by current HEAD blame commit
        let mut raw_groups = Vec::new();
        let mut group_start = entries[0].final_line;
        let mut group_end = entries[0].final_line;
        let mut current_sha = entries[0].sha.clone();

        for entry in entries.iter().skip(1) {
            if entry.sha == current_sha {
                group_end = entry.final_line;
            } else {
                raw_groups.push((group_start, group_end, current_sha.clone()));
                group_start = entry.final_line;
                group_end = entry.final_line;
                current_sha.clone_from(&entry.sha);
            }
        }
        raw_groups.push((group_start, group_end, current_sha));

        // 2. Trace genesis for each blame group
        let mut resolved_slices = Vec::new();
        for (start, end, blame_sha) in raw_groups {
            let slice = self.resolve_slice_genesis(rel_path, start, end, &blame_sha)?;
            resolved_slices.push(slice);
        }

        // 3. Coalesce adjacent contiguous slices that share the exact same genesis_sha and fallback status
        let mut coalesced: Vec<OriginSlice> = Vec::new();
        for slice in resolved_slices {
            if let Some(last) = coalesced.last_mut() {
                if last.line_end + 1 == slice.line_start
                    && last.genesis_sha == slice.genesis_sha
                    && last.fallback_used == slice.fallback_used
                {
                    last.line_end = slice.line_end;
                    for b_sha in slice.blame_commits {
                        if !last.blame_commits.contains(&b_sha) {
                            last.blame_commits.push(b_sha);
                        }
                    }
                    continue;
                }
            }
            coalesced.push(slice);
        }

        Ok(coalesced)
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

        let blame_short = if fallback_sha.len() >= 7 {
            fallback_sha[..7].to_string()
        } else {
            fallback_sha.to_string()
        };

        Ok(OriginSlice {
            line_start: start,
            line_end: end,
            genesis_sha: genesis_commit.sha,
            short_sha: genesis_commit.short_sha,
            blame_sha: fallback_sha.to_string(),
            blame_short_sha: blame_short,
            blame_commits: vec![fallback_sha.to_string()],
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

        // Case 3: Substantive commit body (>20 chars of actual descriptive rationale)
        if let Some(substantive) = Self::extract_substantive_commit_body(&commit.body) {
            return (substantive, Confidence::Medium);
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

    /// Extract substantive explanatory content from a commit body, filtering out automated
    /// merge headers, standard Git trailers, and noise.
    pub fn extract_substantive_commit_body(body: &str) -> Option<String> {
        let mut substantive_lines = Vec::new();

        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_lowercase();

            // 1. Filter auto-generated Git merge headers
            if lower.starts_with("merge branch ")
                || lower.starts_with("merge remote-tracking branch ")
                || lower.starts_with("merge tag ")
                || lower.starts_with("merge pull request ")
                || lower.starts_with("merged in ")
            {
                continue;
            }

            // 2. Filter Git trailers and metadata
            if lower.starts_with("signed-off-by:")
                || lower.starts_with("co-authored-by:")
                || lower.starts_with("reviewed-by:")
                || lower.starts_with("acked-by:")
                || lower.starts_with("see merge request ")
                || lower.starts_with("change-id:")
                || lower.starts_with("git-dns-id:")
                || lower.starts_with("fixes:")
                || lower.starts_with("resolves:")
                || lower.starts_with("closes:")
            {
                continue;
            }

            // 3. Filter markdown dividers
            if trimmed.chars().all(|c| c == '-' || c == '=' || c == '*') && trimmed.len() >= 3 {
                continue;
            }

            substantive_lines.push(trimmed);
        }

        if substantive_lines.is_empty() {
            return None;
        }

        let combined = substantive_lines.join(" ");
        let trimmed = combined.trim();

        // Must be >20 chars and contain at least 3 words to qualify as a substantive explanation
        if trimmed.len() > 20 && trimmed.split_whitespace().count() >= 3 {
            let lower = trimmed.to_lowercase();
            if lower.starts_with("into ") || lower.starts_with("from ") {
                return None;
            }
            Some(trimmed.to_string())
        } else {
            None
        }
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
