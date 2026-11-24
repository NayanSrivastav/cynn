use crate::git::PlatformKind;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Detailed record of why a candidate PR or MR was disqualified during forensic correlation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RejectedPrDecision {
    pub reference: String,
    pub title: String,
    pub url: String,
    pub reason: String,
}

impl RejectedPrDecision {
    pub fn audit_message(&self) -> String {
        format!(
            "Linked PR {} (\"{}\") rejected because {}",
            self.reference, self.title, self.reason
        )
    }
}

/// Structure representing a candidate Pull Request or Merge Request for correlation
#[derive(Debug, Clone)]
pub struct RemotePrCandidate {
    pub id: u64,
    pub title: String,
    pub description: Option<String>,
    pub source_branch: Option<String>,
    pub url: String,
    pub author: Option<String>,
    /// True when the platform's own commit-to-PR/MR endpoint returned this candidate, which means the
    /// platform has already verified that the commit is physically inside it.
    pub linked_by_platform: bool,
}

/// Score for an MR whose Jira key matches the commit's key (or issue reference) exactly.
pub const SCORE_EXACT_KEY: i32 = 100;
/// Score for an MR the platform links to the commit, but whose Jira key differs from the commit's
/// key or where the commit carries no Jira key at all.
pub const SCORE_PLATFORM_ASSOCIATED: i32 = 30;

/// Why an MR is only an indirect association with a commit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssociationKind {
    /// The commit has no Jira key, so the MR supplies the ticket. The MR is usually the feature MR.
    MissingCommitKey,
    /// The commit and the MR carry different Jira keys. The MR is often a bundle (hotfix, merge train)
    /// whose description covers other work.
    KeyDiscrepancy,
}

/// An indirect MR association with a human-readable note.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrAssociation {
    pub kind: AssociationKind,
    pub note: String,
}

/// (score, candidate, reject reason, association for indirect matches)
type Scored<'a> = (
    i32,
    &'a RemotePrCandidate,
    Option<String>,
    Option<PrAssociation>,
);

/// The selected PR/MR together with how it was matched.
#[derive(Debug, Clone)]
pub struct PrMatch<'a> {
    pub candidate: &'a RemotePrCandidate,
    pub score: i32,
    /// Set only for an indirect match (`SCORE_PLATFORM_ASSOCIATED`): describes why the MR is associated.
    pub association: Option<PrAssociation>,
}

/// Extract issue keys (e.g., Jira keys like "COLL-7431", "PROJ-123", or issue references "#1044", "!1044")
pub fn extract_issue_keys(text: &str) -> Vec<String> {
    let mut keys = Vec::new();

    for token in text.split(|c: char| {
        c.is_whitespace()
            || c == '/'
            || c == ':'
            || c == '('
            || c == ')'
            || c == '['
            || c == ']'
            || c == ','
    }) {
        let clean = token.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '-' && c != '_' && c != '#' && c != '!'
        });

        // 1. Issue numbers: #1234 or !1234
        if let Some(rest) = clean.strip_prefix('#').or_else(|| clean.strip_prefix('!')) {
            if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
                let key = format!("#{rest}");
                if !keys.contains(&key) {
                    keys.push(key);
                }
                continue;
            }
        }

        // 2. Jira / Issue tracker keys: e.g. "COLL-7431", "ABC_DEF-99", "PROJ-1"
        // Pattern: [A-Za-z][A-Za-z0-9_]{1,15}-\d+
        if let Some(dash_idx) = clean.find('-') {
            let prefix = &clean[..dash_idx];
            let suffix = &clean[dash_idx + 1..];

            let valid_prefix = !prefix.is_empty()
                && prefix.len() <= 15
                && prefix.starts_with(|c: char| c.is_ascii_alphabetic())
                && prefix
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_');

            let valid_suffix = !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit());

            if valid_prefix && valid_suffix {
                let normalized = format!("{}-{}", prefix.to_ascii_uppercase(), suffix);
                if !keys.contains(&normalized) {
                    keys.push(normalized);
                }
            }
        }
    }

    keys
}

/// Score candidate PRs/MRs against commit metadata and return the best match (if any) alongside an
/// audit trail of rejected candidates.
///
/// Tiers:
/// - `SCORE_EXACT_KEY` (100): the commit's key appears in the MR title, branch or description.
/// - `SCORE_PLATFORM_ASSOCIATED` (30): the platform's commit endpoint returned the MR, so the commit is
///   physically inside it, but the Jira keys differ or the commit has none. Accepted as an indirect
///   association, never as an exact match.
/// - `-1`: key conflict on a candidate the platform did not link, or an associated MR that loses to an
///   exact-key candidate.
#[allow(clippy::too_many_lines)]
pub fn select_best_pr_match_with_audit<'a>(
    candidates: &'a [RemotePrCandidate],
    commit_subject: &str,
    commit_body: &str,
    platform: PlatformKind,
) -> (Option<PrMatch<'a>>, Vec<RejectedPrDecision>) {
    if candidates.is_empty() {
        return (None, Vec::new());
    }

    let full_commit_text = format!("{commit_subject}\n{commit_body}");
    let commit_keys = extract_issue_keys(&full_commit_text);
    let commit_jira_keys: Vec<String> = commit_keys
        .iter()
        .filter(|k| !k.starts_with('#') && !k.starts_with('!'))
        .cloned()
        .collect();

    let mut scored_candidates: Vec<Scored<'a>> = Vec::new();

    let ref_prefix = match platform {
        PlatformKind::GitLab => "!",
        _ => "#",
    };

    for candidate in candidates {
        let candidate_title_keys = extract_issue_keys(&candidate.title);
        let candidate_branch_keys = candidate
            .source_branch
            .as_deref()
            .map(extract_issue_keys)
            .unwrap_or_default();
        let candidate_desc_keys = candidate
            .description
            .as_deref()
            .map(extract_issue_keys)
            .unwrap_or_default();

        let mut all_candidate_keys = candidate_title_keys.clone();
        for k in candidate_branch_keys
            .iter()
            .chain(candidate_desc_keys.iter())
        {
            if !all_candidate_keys.contains(k) {
                all_candidate_keys.push(k.clone());
            }
        }
        all_candidate_keys.push(format!("{ref_prefix}{}", candidate.id));
        all_candidate_keys.push(format!("#{}", candidate.id));
        all_candidate_keys.push(format!("!{}", candidate.id));

        let candidate_jira_keys: Vec<String> = all_candidate_keys
            .iter()
            .filter(|k| !k.starts_with('#') && !k.starts_with('!'))
            .cloned()
            .collect();

        let (score, reject_reason, association) = if !commit_jira_keys.is_empty() {
            // Commit has explicit Jira key(s)
            let mut matches_commit_key = false;
            for key in &commit_keys {
                if all_candidate_keys.contains(key)
                    || candidate.title.to_ascii_uppercase().contains(key)
                    || candidate
                        .source_branch
                        .as_deref()
                        .map(|b| b.to_ascii_uppercase().contains(key))
                        .unwrap_or(false)
                    || candidate
                        .description
                        .as_deref()
                        .map(|d| d.to_ascii_uppercase().contains(key))
                        .unwrap_or(false)
                {
                    matches_commit_key = true;
                    break;
                }
            }

            if matches_commit_key {
                (SCORE_EXACT_KEY, None, None)
            } else if !candidate_jira_keys.is_empty() {
                let reason = format!(
                    "Jira key '{}' conflicts with commit key '{}'",
                    candidate_jira_keys[0], commit_jira_keys[0]
                );
                if candidate.linked_by_platform {
                    let note = format!(
                        "Associated via {ref_prefix}{} ({}): the platform lists this commit inside the MR, but the commit key {} differs from the MR key",
                        candidate.id, candidate_jira_keys[0], commit_jira_keys[0]
                    );
                    let association = PrAssociation {
                        kind: AssociationKind::KeyDiscrepancy,
                        note,
                    };
                    (SCORE_PLATFORM_ASSOCIATED, Some(reason), Some(association))
                } else {
                    (-1, Some(reason), None)
                }
            } else {
                let reason = format!(
                    "PR does not reference required commit key(s) [{}]",
                    commit_jira_keys.join(", ")
                );
                (0, Some(reason), None)
            }
        } else if !commit_keys.is_empty() {
            // Commit has non-Jira issue key(s) like #123
            let mut matches = false;
            for key in &commit_keys {
                if all_candidate_keys.contains(key) {
                    matches = true;
                    break;
                }
            }
            if matches {
                (SCORE_EXACT_KEY, None, None)
            } else {
                let reason = format!(
                    "PR does not reference required commit key(s) [{}]",
                    commit_keys.join(", ")
                );
                (0, Some(reason), None)
            }
        } else {
            // Commit has no issue keys
            if candidate_jira_keys.is_empty() {
                let sim = score_text_similarity(commit_subject, &candidate.title);
                if sim > 0 {
                    (sim, None, None)
                } else {
                    (
                        sim,
                        Some("Insufficient title similarity to commit".to_string()),
                        None,
                    )
                }
            } else {
                let reason = format!(
                    "PR ticket key '{}' not referenced in commit",
                    candidate_jira_keys[0]
                );
                if candidate.linked_by_platform {
                    // The commit has no Jira key, so the MR supplies the missing ticket key.
                    let note = format!(
                        "Associated via {ref_prefix}{} ({}): the platform lists this commit inside the MR and the commit message carries no Jira key, so the MR supplies the ticket",
                        candidate.id, candidate_jira_keys[0]
                    );
                    let association = PrAssociation {
                        kind: AssociationKind::MissingCommitKey,
                        note,
                    };
                    (SCORE_PLATFORM_ASSOCIATED, Some(reason), Some(association))
                } else {
                    (-1, Some(reason), None)
                }
            }
        };

        scored_candidates.push((score, candidate, reject_reason, association));
    }

    // An associated MR is only a fallback: when another candidate matches the commit key exactly,
    // the associated MR is disqualified.
    if let Some(exact) = scored_candidates
        .iter()
        .find(|(score, ..)| *score == SCORE_EXACT_KEY)
        .map(|(_, cand, ..)| cand.id)
    {
        for entry in &mut scored_candidates {
            if entry.0 == SCORE_PLATFORM_ASSOCIATED {
                let conflict = entry.2.clone().unwrap_or_default();
                entry.0 = -1;
                entry.2 = Some(format!(
                    "{conflict}; superseded by exact key match {ref_prefix}{exact}"
                ));
                entry.3 = None;
            }
        }
    }

    let mut eligible: Vec<&Scored<'a>> = scored_candidates
        .iter()
        .filter(|(score, ..)| *score > 0)
        .collect();
    eligible.sort_by_key(|a| std::cmp::Reverse(a.0));

    let best = eligible.first().map(|(score, cand, _, assoc)| PrMatch {
        candidate: cand,
        score: *score,
        association: assoc.clone(),
    });

    let mut rejected = Vec::new();
    for (score, candidate, reason, association) in scored_candidates {
        if let Some(ref best_match) = best {
            if best_match.candidate.id == candidate.id {
                continue;
            }
        }
        let reason = if association.is_some() {
            // An associated MR that lost to a higher-scoring candidate that is not an exact match.
            let winner = best.as_ref().map_or(0, |b| b.candidate.id);
            Some(format!("Associated MR outranked by {ref_prefix}{winner}"))
        } else {
            reason
        };
        let fallback_reason = if score < 0 {
            "Disqualified due to issue key conflict"
        } else {
            "PR did not meet minimum similarity threshold"
        };
        rejected.push(RejectedPrDecision {
            reference: format!("{ref_prefix}{}", candidate.id),
            title: candidate.title.clone(),
            url: candidate.url.clone(),
            reason: reason.unwrap_or_else(|| fallback_reason.to_string()),
        });
    }

    (best, rejected)
}

/// Same as [`select_best_pr_match_with_audit`] but returns only the candidate.
pub fn select_best_pr_candidate_with_audit<'a>(
    candidates: &'a [RemotePrCandidate],
    commit_subject: &str,
    commit_body: &str,
    platform: PlatformKind,
) -> (Option<&'a RemotePrCandidate>, Vec<RejectedPrDecision>) {
    let (best, rejected) =
        select_best_pr_match_with_audit(candidates, commit_subject, commit_body, platform);
    (best.map(|m| m.candidate), rejected)
}

/// Disqualify or score candidate PRs/MRs against commit metadata to guarantee zero false attribution.
pub fn select_best_pr_candidate<'a>(
    candidates: &'a [RemotePrCandidate],
    commit_subject: &str,
    commit_body: &str,
) -> Option<&'a RemotePrCandidate> {
    select_best_pr_candidate_with_audit(
        candidates,
        commit_subject,
        commit_body,
        PlatformKind::Generic,
    )
    .0
}

fn score_text_similarity(subject: &str, title: &str) -> i32 {
    let clean_subj = clean_title_noise(subject);
    let clean_cand = clean_title_noise(title);

    if clean_subj.is_empty() || clean_cand.is_empty() {
        return 0;
    }

    if clean_subj == clean_cand {
        return 90;
    }

    if clean_subj.contains(&clean_cand) || clean_cand.contains(&clean_subj) {
        return 70;
    }

    // Token overlap
    let subj_words: HashSet<&str> = clean_subj
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .collect();
    let cand_words: HashSet<&str> = clean_cand
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .collect();

    if subj_words.is_empty() || cand_words.is_empty() {
        return 0;
    }

    let common = subj_words.intersection(&cand_words).count();
    if common >= 2 {
        50
    } else {
        0
    }
}

fn clean_title_noise(text: &str) -> String {
    let lower = text.to_lowercase();
    let mut trimmed = lower.as_str();

    // Strip conventional commit prefixes
    for prefix in &[
        "feat:",
        "fix:",
        "chore:",
        "docs:",
        "refactor:",
        "perf:",
        "test:",
        "style:",
        "build:",
        "ci:",
        "feat(",
        "fix(",
        "chore(",
        "docs(",
        "refactor(",
    ] {
        if trimmed.starts_with(prefix) {
            if let Some(idx) = trimmed.find(':') {
                trimmed = trimmed[idx + 1..].trim();
            }
            break;
        }
    }

    trimmed
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_jira_and_git_keys() {
        let text = "COLL-7431: Notetaker legacy filter migration (!1044, #42, PROJ_1-99).";
        let keys = extract_issue_keys(text);
        assert_eq!(keys, vec!["COLL-7431", "#1044", "#42", "PROJ_1-99"]);
    }

    #[test]
    fn test_disqualify_conflicting_jira_key_when_not_platform_linked() {
        let commit_subject = "COLL-7431: Notetaker legacy filter to new filter migration API";
        let candidates = vec![RemotePrCandidate {
            id: 1097,
            title: "COLL-7406: Stream zip extraction through ranged S3 reads".to_string(),
            description: Some("Extensive zip extraction rationale".to_string()),
            source_branch: Some("hotfix/0.0.61.2".to_string()),
            url: "https://gitlab.com/.../1097".to_string(),
            author: Some("Nayansr".to_string()),
            linked_by_platform: false,
        }];

        let selected = select_best_pr_candidate(&candidates, commit_subject, "");
        assert!(
            selected.is_none(),
            "A conflicting Jira key on an MR the platform did not link must be disqualified"
        );
    }

    #[test]
    fn test_match_correct_jira_key() {
        let commit_subject = "COLL-7406: Stream zip extraction through ranged S3 reads";
        let candidates = vec![
            RemotePrCandidate {
                id: 1044,
                title: "COLL-6884: Protect branch".to_string(),
                description: None,
                source_branch: None,
                url: "https://gitlab.com/.../1044".to_string(),
                author: None,
                linked_by_platform: true,
            },
            RemotePrCandidate {
                id: 1097,
                title: "COLL-7406: Stream zip extraction through ranged S3 reads".to_string(),
                description: Some("Rationale".to_string()),
                source_branch: Some("hotfix/0.0.61.2".to_string()),
                url: "https://gitlab.com/.../1097".to_string(),
                author: None,
                linked_by_platform: true,
            },
        ];

        let selected = select_best_pr_candidate(&candidates, commit_subject, "").unwrap();
        assert_eq!(selected.id, 1097);
    }

    #[test]
    fn test_match_issue_reference_in_commit_body() {
        let commit_subject = "added branch to be protected";
        let commit_body = "See merge request !1044";
        let candidates = vec![RemotePrCandidate {
            id: 1044,
            title: "Hotfix/0.0.56.2".to_string(),
            description: None,
            source_branch: None,
            url: "https://gitlab.com/.../1044".to_string(),
            author: None,
            linked_by_platform: true,
        }];

        let selected = select_best_pr_candidate(&candidates, commit_subject, commit_body).unwrap();
        assert_eq!(selected.id, 1044);
    }

    #[test]
    fn test_match_title_similarity_without_keys() {
        let commit_subject = "fix: prevent deadlock in channel dispatcher";
        let candidates = vec![RemotePrCandidate {
            id: 55,
            title: "Prevent deadlock in channel dispatcher".to_string(),
            description: None,
            source_branch: None,
            url: "https://github.com/.../55".to_string(),
            author: None,
            linked_by_platform: true,
        }];

        let selected = select_best_pr_candidate(&candidates, commit_subject, "").unwrap();
        assert_eq!(selected.id, 55);
    }

    fn candidate(id: u64, title: &str, desc: Option<&str>, linked: bool) -> RemotePrCandidate {
        RemotePrCandidate {
            id,
            title: title.to_string(),
            description: desc.map(String::from),
            source_branch: None,
            url: format!("https://gitlab.com/repo/-/merge_requests/{id}"),
            author: Some("developer".to_string()),
            linked_by_platform: linked,
        }
    }

    #[test]
    fn test_platform_linked_mr_with_key_conflict_is_associated_not_rejected() {
        let candidates = vec![candidate(
            1097,
            "Hotfix/0.0.56.2",
            Some("Backport COLL-7406 zip extraction"),
            true,
        )];

        let (best, rejected) = select_best_pr_match_with_audit(
            &candidates,
            "feature/COLL-7431",
            "",
            PlatformKind::GitLab,
        );

        let best = best.expect("platform-linked MR must be accepted as an association");
        assert_eq!(best.candidate.id, 1097);
        assert_eq!(best.score, SCORE_PLATFORM_ASSOCIATED);
        let assoc = best.association.expect("association note");
        assert_eq!(assoc.kind, AssociationKind::KeyDiscrepancy);
        let note = assoc.note;
        assert!(
            note.starts_with("Associated via !1097 (COLL-7406)"),
            "{note}"
        );
        assert!(note.contains("COLL-7431"), "{note}");
        assert_eq!(rejected, Vec::new());
    }

    #[test]
    fn test_platform_linked_mr_supplies_missing_commit_key() {
        let candidates = vec![candidate(
            608,
            "COLL-7329: split qTest logs into ranged files",
            Some("Write incremental logs as ranged files"),
            true,
        )];

        let (best, rejected) =
            select_best_pr_match_with_audit(&candidates, "temp", "", PlatformKind::GitLab);

        let best = best.expect("MR must supply the missing ticket key");
        assert_eq!(best.score, SCORE_PLATFORM_ASSOCIATED);
        let assoc = best.association.expect("association note");
        assert_eq!(assoc.kind, AssociationKind::MissingCommitKey);
        let note = assoc.note;
        assert!(
            note.starts_with("Associated via !608 (COLL-7329)"),
            "{note}"
        );
        assert!(note.contains("no Jira key"), "{note}");
        assert_eq!(rejected, Vec::new());
    }

    #[test]
    fn test_no_commit_key_and_mr_not_platform_linked_is_still_rejected() {
        let candidates = vec![candidate(608, "COLL-7329: split logs", None, false)];

        let (best, rejected) =
            select_best_pr_match_with_audit(&candidates, "temp", "", PlatformKind::GitLab);

        assert!(best.is_none());
        assert_eq!(
            rejected[0].reason,
            "PR ticket key 'COLL-7329' not referenced in commit"
        );
    }

    #[test]
    fn test_exact_key_match_beats_and_disqualifies_associated_mr() {
        let candidates = vec![
            candidate(1097, "Hotfix/0.0.56.2", Some("Backport COLL-7406"), true),
            candidate(1044, "COLL-7431: Notetaker migration", None, true),
        ];

        let (best, rejected) = select_best_pr_match_with_audit(
            &candidates,
            "COLL-7431: Notetaker migration API",
            "",
            PlatformKind::GitLab,
        );

        let best = best.unwrap();
        assert_eq!(best.candidate.id, 1044);
        assert_eq!(best.score, SCORE_EXACT_KEY);
        assert!(best.association.is_none());
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].reference, "!1097");
        assert_eq!(
            rejected[0].reason,
            "Jira key 'COLL-7406' conflicts with commit key 'COLL-7431'; superseded by exact key match !1044"
        );
    }

    #[test]
    fn test_two_associated_mrs_keep_first_and_report_the_other() {
        let candidates = vec![
            candidate(10, "COLL-1: first", None, true),
            candidate(11, "COLL-2: second", None, true),
        ];

        let (best, rejected) =
            select_best_pr_match_with_audit(&candidates, "temp", "", PlatformKind::GitLab);

        assert_eq!(best.unwrap().candidate.id, 10);
        assert_eq!(rejected.len(), 1);
        assert_eq!(rejected[0].reason, "Associated MR outranked by !10");
    }
}
