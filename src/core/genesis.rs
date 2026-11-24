use crate::core::accordion::AccordionSnippet;
use crate::core::Target;
use crate::enrich::github::PrDetails;
use crate::enrich::match_pr::AssociationKind;
pub use crate::enrich::match_pr::RejectedPrDecision;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genesis_diff: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_prs: Vec<RejectedPrDecision>,
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
        self.analyze_with_options(target, false)
    }

    /// Analyze the target with optional flags (e.g., `include_diff`)
    #[allow(clippy::too_many_lines, clippy::missing_panics_doc)]
    pub fn analyze_with_options(
        &self,
        target: &Target,
        include_diff: bool,
    ) -> Result<LineageDossier, String> {
        let (start_line, end_line, function_resolved) = if let Some(ref func) = target.function_name
        {
            let file_lines = self.repo.read_file_lines(&target.path)?;
            let (s, e) = crate::core::target::resolve_function_range(&file_lines, func)?;
            (s, e, Some(func.clone()))
        } else {
            let start = target.start_line.unwrap_or(1);
            let end = target.end_line.unwrap_or(start);
            (start, end, None)
        };

        // 1. Initial porcelain blame: get both genesis blame (-w -C -C -M) and HEAD blame
        let (genesis_entries, head_entries) =
            self.repo
                .blame_lines_with_head(&target.path, start_line, end_line)?;

        if genesis_entries.is_empty() {
            return Err("No blame information available for specified lines".to_string());
        }

        // 2. Extract code snippet
        let code_snippet = genesis_entries
            .iter()
            .map(|e| e.code.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // 3. Multi-Origin Dissection: Group contiguous lines by their introducing genesis commit SHA
        let mut origin_slices = self.dissect_origins(&genesis_entries, &head_entries);

        // Determine multi-origin status based on DISTINCT resolved introducing commits (genesis_sha),
        // not merely the count of contiguous blame slices. Multiple slices originating from the
        // same introducing commit belong to a single logical origin.
        let mut distinct_genesis_shas: Vec<String> = origin_slices
            .iter()
            .map(|s| s.genesis_sha.clone())
            .collect();
        distinct_genesis_shas.sort();
        distinct_genesis_shas.dedup();
        let is_multi_origin = distinct_genesis_shas.len() > 1;

        if !is_multi_origin && origin_slices.len() > 1 {
            let first = origin_slices[0].clone();
            let last_end = origin_slices
                .last()
                .map(|s| s.line_end)
                .unwrap_or(first.line_end);
            let mut all_blames = Vec::new();
            for s in &origin_slices {
                for b in &s.blame_commits {
                    if !all_blames.contains(b) {
                        all_blames.push(b.clone());
                    }
                }
            }
            origin_slices = vec![OriginSlice {
                line_start: origin_slices[0].line_start,
                line_end: last_end,
                genesis_sha: first.genesis_sha,
                short_sha: first.short_sha,
                blame_sha: first.blame_sha,
                blame_short_sha: first.blame_short_sha,
                blame_commits: all_blames,
                author: first.author,
                date: first.date,
                summary: first.summary,
                history_resolved: first.history_resolved,
                fallback_used: first.fallback_used,
            }];
        }

        // 4. Line History Reconstruction: Walk backward across revisions to find true genesis
        let (genesis_commit, milestones, provenance_fallback) = self.trace_provenance(
            &target.path,
            start_line,
            end_line,
            &origin_slices,
            is_multi_origin,
        )?;

        if !is_multi_origin {
            if let Some(ref gc) = genesis_commit {
                if let Some(slice) = origin_slices.first_mut() {
                    slice.genesis_sha.clone_from(&gc.sha);
                    slice.short_sha.clone_from(&gc.short_sha);
                    slice.author.clone_from(&gc.author_name);
                    slice.date.clone_from(&gc.author_date);
                    slice.summary.clone_from(&gc.subject);
                }
            }
        }

        let fallback_used = provenance_fallback || origin_slices.iter().any(|s| s.fallback_used);

        // 5. Context Enrichment & Origin Attribution
        let (final_genesis, intent_summary, confidence, pr_ref, pr_details, rejected_prs) =
            if is_multi_origin {
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
                (None, summary, conf, None, None, Vec::new())
            } else {
                let remote_info = self.repo.get_remote_info();
                let single_genesis_commit =
                    genesis_commit.expect("Single-origin block must have genesis commit");

                let mut pr_ref = Self::extract_pr_reference(
                    &single_genesis_commit.subject,
                    &single_genesis_commit.body,
                );
                let (mut pr_details, mut rejected_prs) = resolve_remote_context(
                    &self.repo.root,
                    remote_info.as_ref(),
                    &single_genesis_commit,
                );

                if pr_details.is_none() {
                    for m in milestones
                        .iter()
                        .rev()
                        .filter(|m| m.commit_sha != single_genesis_commit.sha)
                        .take(3)
                    {
                        if let Ok(m_commit) = self.repo.get_commit(&m.commit_sha) {
                            let (details, rejs) = resolve_remote_context(
                                &self.repo.root,
                                remote_info.as_ref(),
                                &m_commit,
                            );
                            for r in rejs {
                                if !rejected_prs.contains(&r) {
                                    rejected_prs.push(r);
                                }
                            }
                            if let Some(details) = details {
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
                    rejected_prs,
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

        let genesis_diff = if include_diff {
            if let Some(ref commit) = final_genesis {
                self.repo.get_commit_diff(&commit.sha, &target.path).ok()
            } else if is_multi_origin {
                let mut combined = String::new();
                for sha in &distinct_genesis_shas {
                    if let Ok(d) = self.repo.get_commit_diff(sha, &target.path) {
                        if !d.is_empty() {
                            if !combined.is_empty() {
                                combined.push_str("\n\n");
                            }
                            combined.push_str(&d);
                        }
                    }
                }
                if combined.is_empty() {
                    None
                } else {
                    Some(combined)
                }
            } else {
                None
            }
        } else {
            None
        };

        let line_range = if let Some(ref func) = function_resolved {
            if start_line == end_line {
                format!("{start_line} (function: {func})")
            } else {
                format!("{start_line}-{end_line} (function: {func})")
            }
        } else if start_line == end_line {
            format!("{start_line}")
        } else {
            format!("{start_line}-{end_line}")
        };

        Ok(LineageDossier {
            file_path: target.path.display().to_string(),
            line_range,
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
            genesis_diff,
            rejected_prs,
        })
    }

    /// Dissect a line range into contiguous slices based on introducing genesis commit SHA
    fn dissect_origins(
        &self,
        genesis_entries: &[crate::git::BlameEntry],
        head_entries: &[crate::git::BlameEntry],
    ) -> Vec<OriginSlice> {
        if genesis_entries.is_empty() {
            return Vec::new();
        }

        let mut raw_slices: Vec<OriginSlice> = Vec::new();
        let mut group_start = genesis_entries[0].final_line;
        let mut group_end = genesis_entries[0].final_line;
        let mut current_genesis_sha = genesis_entries[0].sha.clone();
        let mut current_blames: Vec<String> = Vec::new();

        if let Some(h) = head_entries.first() {
            current_blames.push(h.sha.clone());
        } else {
            current_blames.push(current_genesis_sha.clone());
        }

        for (i, entry) in genesis_entries.iter().enumerate().skip(1) {
            let head_sha = head_entries
                .get(i)
                .map(|h| h.sha.clone())
                .unwrap_or_else(|| entry.sha.clone());

            if entry.sha == current_genesis_sha {
                group_end = entry.final_line;
                if !current_blames.contains(&head_sha) {
                    current_blames.push(head_sha);
                }
            } else {
                let slice = self.build_origin_slice(
                    group_start,
                    group_end,
                    &current_genesis_sha,
                    &current_blames,
                );
                raw_slices.push(slice);

                group_start = entry.final_line;
                group_end = entry.final_line;
                current_genesis_sha.clone_from(&entry.sha);
                current_blames = vec![head_sha];
            }
        }

        let slice = self.build_origin_slice(
            group_start,
            group_end,
            &current_genesis_sha,
            &current_blames,
        );
        raw_slices.push(slice);

        raw_slices
    }

    fn build_origin_slice(
        &self,
        start: usize,
        end: usize,
        genesis_sha: &str,
        blame_commits: &[String],
    ) -> OriginSlice {
        let commit = self
            .repo
            .get_commit(genesis_sha)
            .unwrap_or_else(|_| CommitInfo {
                sha: genesis_sha.to_string(),
                short_sha: if genesis_sha.len() >= 7 {
                    genesis_sha[..7].to_string()
                } else {
                    genesis_sha.to_string()
                },
                author_name: "Unknown".to_string(),
                author_email: String::new(),
                author_date: String::new(),
                subject: String::new(),
                body: String::new(),
            });

        let primary_blame = blame_commits
            .first()
            .cloned()
            .unwrap_or_else(|| genesis_sha.to_string());
        let blame_short = if primary_blame.len() >= 7 {
            primary_blame[..7].to_string()
        } else {
            primary_blame.clone()
        };

        OriginSlice {
            line_start: start,
            line_end: end,
            genesis_sha: commit.sha,
            short_sha: commit.short_sha,
            blame_sha: primary_blame,
            blame_short_sha: blame_short,
            blame_commits: blame_commits.to_vec(),
            author: commit.author_name,
            date: commit.author_date,
            summary: commit.subject,
            history_resolved: true,
            fallback_used: false,
        }
    }

    /// Trace the full line history across revisions to reconstruct evolution milestones
    #[allow(clippy::too_many_lines)]
    fn trace_provenance(
        &self,
        rel_path: &std::path::Path,
        start_line: usize,
        end_line: usize,
        origin_slices: &[OriginSlice],
        is_multi_origin: bool,
    ) -> Result<(Option<CommitInfo>, Vec<EvolutionMilestone>, bool), String> {
        let commits_history = self
            .repo
            .trace_line_evolution(rel_path, start_line, end_line, true)
            .ok();

        if is_multi_origin {
            // For multi-origin blocks: no single commit in the aggregate timeline is the sole genesis;
            // individual origin slices are authoritative.
            let milestones = if let Some(history) = commits_history {
                // Find the oldest commit in history that is referenced by any origin slice
                let last_slice_idx = history.iter().rposition(|c| {
                    origin_slices.iter().any(|s| {
                        s.genesis_sha == c.sha
                            || s.short_sha == c.short_sha
                            || s.blame_commits.contains(&c.sha)
                    })
                });

                let filtered_history: Vec<_> = if let Some(last_idx) = last_slice_idx {
                    history.into_iter().take(last_idx + 1).collect()
                } else {
                    history
                };

                filtered_history
                    .into_iter()
                    .map(|commit| {
                        let pr_ref = Self::extract_pr_reference(&commit.subject, &commit.body);
                        EvolutionMilestone {
                            date: commit.author_date,
                            commit_sha: commit.sha,
                            short_sha: commit.short_sha,
                            author: commit.author_name,
                            summary: commit.subject,
                            pr_reference: pr_ref,
                            is_genesis: false,
                            history_resolved: true,
                            fallback_used: false,
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            return Ok((None, milestones, false));
        }

        // Single-origin block: Walk history to find earliest genesis milestone
        if let Some(history) = commits_history {
            if !history.is_empty() {
                let initial_genesis_sha = origin_slices.first().map(|s| s.genesis_sha.as_str());
                let (genesis, is_cross_file_blame) = if let Some(b_sha) = initial_genesis_sha {
                    if history
                        .iter()
                        .any(|c| c.sha == b_sha || c.short_sha == b_sha)
                    {
                        (history.last().unwrap().clone(), false)
                    } else if let Ok(blame_commit) = self.repo.get_commit(b_sha) {
                        // The blame commit comes from a cross-file copy (-C -C) that log -L did not follow
                        (blame_commit, true)
                    } else {
                        (history.last().unwrap().clone(), false)
                    }
                } else {
                    (history.last().unwrap().clone(), false)
                };

                let total = history.len();
                let mut milestones = Vec::new();
                for (idx, commit) in history.into_iter().enumerate() {
                    let is_genesis = !is_cross_file_blame && idx == total - 1;
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

                if is_cross_file_blame {
                    let pr_ref = Self::extract_pr_reference(&genesis.subject, &genesis.body);
                    milestones.push(EvolutionMilestone {
                        date: genesis.author_date.clone(),
                        commit_sha: genesis.sha.clone(),
                        short_sha: genesis.short_sha.clone(),
                        author: genesis.author_name.clone(),
                        summary: genesis.subject.clone(),
                        pr_reference: pr_ref,
                        is_genesis: true,
                        history_resolved: true,
                        fallback_used: false,
                    });
                }

                return Ok((Some(genesis), milestones, false));
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
            history_resolved: primary_slice.history_resolved,
            fallback_used: true,
        }];

        Ok((Some(commit), milestones, true))
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
            // An associated MR is only an indirect link, so it never reaches High confidence.
            // - MissingCommitKey: the MR is usually the feature MR the commit came from, so its
            //   description is used, capped at Medium.
            // - KeyDiscrepancy: the MR is often a bundle (hotfix, merge train) whose description covers
            //   other work, so its description is not used as the intent. The commit body is tried first.
            let kind = pr.association.as_ref().map(|a| a.kind);
            let use_pr_rationale = kind != Some(AssociationKind::KeyDiscrepancy);
            let (pr_label, pr_confidence) = if kind.is_some() {
                (format!("{} [associated MR]", pr.title), Confidence::Medium)
            } else {
                (pr.title.clone(), Confidence::High)
            };

            // Case 1: Substantive PR description
            if let Some(summary) = pr.body_summary.as_ref().filter(|_| use_pr_rationale) {
                if summary.trim().len() > 20 {
                    let full_intent =
                        if let Some(h) = pr.reviewer_highlights.iter().find(|h| h.is_technical) {
                            format!("{} — Technical Review ({}): {}", summary, h.author, h.body)
                        } else {
                            summary.clone()
                        };
                    return (format!("{pr_label}: {full_intent}"), pr_confidence);
                }
            }

            // Case 2: Substantive technical reviewer rationale
            if use_pr_rationale {
                if let Some(h) = pr.reviewer_highlights.iter().find(|h| h.is_technical) {
                    return (
                        format!("{pr_label}: Technical Review ({}): {}", h.author, h.body),
                        pr_confidence,
                    );
                }
            }
        }

        // Case 3: Substantive commit body (>20 chars of actual descriptive rationale)
        if let Some(substantive) = Self::extract_substantive_commit_body(&commit.body) {
            return (substantive, Confidence::Medium);
        }

        // Case 4: PR title only (low confidence)
        if let Some(pr) = pr_details {
            let tag = if pr.association.is_some() {
                " [associated MR]"
            } else {
                ""
            };
            return (
                format!("PR #{}: {}{tag}", pr.number, pr.title),
                Confidence::Low,
            );
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
                    evidence_type: if pr.association.is_some() {
                        "associated_pull_request_body".to_string()
                    } else {
                        "pull_request_body".to_string()
                    },
                    source: if pr.association.is_some() {
                        format!("PR #{} (associated)", pr.number)
                    } else {
                        format!("PR #{}", pr.number)
                    },
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
                    source: if pr.association.is_some() {
                        format!("PR #{} (associated)", pr.number)
                    } else {
                        format!("PR #{}", pr.number)
                    },
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

#[cfg(test)]
mod association_confidence_tests {
    use super::*;
    use crate::enrich::match_pr::PrAssociation;

    fn commit(subject: &str, body: &str) -> CommitInfo {
        CommitInfo {
            sha: "a".repeat(40),
            short_sha: "aaaaaaaa".to_string(),
            author_name: "dev".to_string(),
            author_email: "dev@example.com".to_string(),
            author_date: "2026-11-01".to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
        }
    }

    fn pr(association: Option<AssociationKind>) -> PrDetails {
        PrDetails {
            number: 608,
            title: "COLL-7329: split qTest logs".to_string(),
            url: "https://gitlab.com/x/-/merge_requests/608".to_string(),
            author: None,
            body_summary: Some(
                "Write incremental logs as ranged files with a lookback window".to_string(),
            ),
            reviewer_highlights: Vec::new(),
            linked_issues: Vec::new(),
            all_matched_prs: vec![608],
            association: association.map(|kind| PrAssociation {
                kind,
                note: "Associated via !608 (COLL-7329)".to_string(),
            }),
        }
    }

    #[test]
    fn exact_match_keeps_high_confidence() {
        let (intent, conf) = GenesisAnalyzer::evaluate_intent_and_confidence_inner(
            &commit("COLL-7329: split logs", ""),
            Some(&pr(None)),
        );
        assert_eq!(conf, Confidence::High);
        assert!(!intent.contains("[associated MR]"));
    }

    #[test]
    fn missing_commit_key_association_uses_mr_body_at_medium() {
        let (intent, conf) = GenesisAnalyzer::evaluate_intent_and_confidence_inner(
            &commit("temp", ""),
            Some(&pr(Some(AssociationKind::MissingCommitKey))),
        );
        assert_eq!(conf, Confidence::Medium);
        assert!(intent.contains("[associated MR]"));
        assert!(intent.contains("ranged files"));
    }

    #[test]
    fn key_discrepancy_association_does_not_use_mr_body_as_intent() {
        let (intent, conf) = GenesisAnalyzer::evaluate_intent_and_confidence_inner(
            &commit(
                "COLL-7391: fix",
                "Take field names from the first records when definitions are empty",
            ),
            Some(&pr(Some(AssociationKind::KeyDiscrepancy))),
        );
        assert_eq!(conf, Confidence::Medium);
        assert!(intent.contains("Take field names"));
        assert!(!intent.contains("ranged files"));
    }

    #[test]
    fn key_discrepancy_without_commit_body_falls_to_low_title_only() {
        let (intent, conf) = GenesisAnalyzer::evaluate_intent_and_confidence_inner(
            &commit("COLL-7391: fix", ""),
            Some(&pr(Some(AssociationKind::KeyDiscrepancy))),
        );
        assert_eq!(conf, Confidence::Low);
        assert!(intent.contains("[associated MR]"));
        assert!(!intent.contains("ranged files"));
    }
}
