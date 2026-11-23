use crate::core::LineageDossier;

/// Render the archaeological dossier as GitHub-flavored Markdown
#[allow(clippy::too_many_lines)]
pub fn render_markdown(dossier: &LineageDossier) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "### Lineage Report: `{}:{}`\n\n",
        dossier.file_path, dossier.line_range
    ));

    if let Some(ref accordion) = dossier.snippet_accordion {
        if let Some(ref scope) = accordion.scope {
            out.push_str(&format!("> **Scope:** `{scope}`\n\n"));
        }

        out.push_str("```\n");
        let gutter_width = accordion.max_line_number.to_string().len().max(2);
        for line in &accordion.lines {
            if line.is_fold_marker {
                out.push_str(&format!(
                    "{:>width$} {} {}\n",
                    "",
                    line.marker,
                    line.content,
                    width = gutter_width
                ));
            } else {
                out.push_str(&format!(
                    "{:>width$} {} {}\n",
                    line.line_number,
                    line.marker,
                    line.content,
                    width = gutter_width
                ));
            }
        }
        out.push_str("```\n\n");
    } else {
        out.push_str("```\n");
        out.push_str(&dossier.code_snippet);
        out.push_str("\n```\n\n");
    }

    // Multi-Origin Breakdown
    if dossier.is_multi_origin {
        let distinct_count = dossier
            .origins
            .iter()
            .map(|s| &s.genesis_sha)
            .collect::<std::collections::HashSet<_>>()
            .len();

        let commit_str = if distinct_count == 1 {
            "1 distinct commit".to_string()
        } else {
            format!("{distinct_count} distinct commits")
        };

        out.push_str("#### Multi-Origin Breakdown\n");
        out.push_str(&format!(
            "> ⚠️ **Note:** This line range contains code introduced across {commit_str}. See the per-slice breakdown below for true line-by-line attribution.\n\n"
        ));
        for slice in &dossier.origins {
            let status = if slice.fallback_used {
                "Fallback Blame (Unverified)"
            } else if slice.history_resolved {
                "Resolved"
            } else {
                "Fallback Blame"
            };
            let mut blame_note = String::new();
            if !slice.blame_commits.is_empty()
                && (slice.blame_commits.len() > 1 || slice.blame_commits[0] != slice.genesis_sha)
            {
                let short_blames: Vec<&str> = slice
                    .blame_commits
                    .iter()
                    .map(|sha| if sha.len() >= 7 { &sha[..7] } else { sha })
                    .collect();
                blame_note = format!(" *(HEAD blame: `{}`)*", short_blames.join("`, `"));
            }
            out.push_str(&format!(
                "- **Lines {}-{}** [{status}]: [`{}`]{blame_note} by {} ({}) — *{}*\n",
                slice.line_start,
                slice.line_end,
                slice.short_sha,
                slice.author,
                slice.date,
                slice.summary
            ));
        }
        out.push('\n');
    }

    if let Some(ref genesis) = dossier.genesis_commit {
        let fallback_note = if dossier.fallback_used {
            " *(Fallback - Unverified DAG)*"
        } else {
            ""
        };
        out.push_str(&format!("#### Genesis (Origin Commit){fallback_note}\n"));
        out.push_str(&format!(
            "- **Commit:** `{}` ({})\n",
            genesis.short_sha, genesis.author_date
        ));
        out.push_str(&format!(
            "- **Author:** {} `<{}>`\n",
            genesis.author_name, genesis.author_email
        ));
        if let Some(ref pr) = dossier.pr_details {
            out.push_str(&format!(
                "- **Pull Request / MR:** [{}]({})\n",
                pr.title, pr.url
            ));
            if !pr.linked_issues.is_empty() {
                out.push_str(&format!(
                    "- **Linked Issues:** {}\n",
                    pr.linked_issues.join(", ")
                ));
            }
            if !pr.reviewer_highlights.is_empty() {
                let h = &pr.reviewer_highlights[0];
                let state = h.review_state.as_deref().unwrap_or("note");
                out.push_str(&format!(
                    "- **Reviewer Note:** {} ({}): {}\n",
                    h.author, state, h.body
                ));
            }
        } else if let Some(ref pr) = dossier.pr_reference {
            out.push_str(&format!("- **Reference:** {pr}\n"));
        }
    } else {
        let distinct_count = dossier
            .origins
            .iter()
            .map(|s| &s.genesis_sha)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let commit_str = if distinct_count == 1 {
            "1 distinct commit".to_string()
        } else {
            format!("{distinct_count} distinct commits")
        };
        out.push_str("#### Multi-Origin Range\n");
        out.push_str(&format!(
            "This code range was introduced across **{commit_str}**. See the Multi-Origin Breakdown above for per-slice provenance.\n"
        ));
    }

    out.push_str(&format!(
        "- **Recorded Intent:** {}\n",
        dossier.intent_summary
    ));
    if !dossier.related_tests.is_empty() {
        out.push_str(&format!(
            "- **Companion Tests:** {}\n",
            dossier.related_tests.join(", ")
        ));
    }
    out.push_str(&format!(
        "- **Evidence Citations:** {} verified sources\n",
        dossier.evidence.len()
    ));
    out.push_str(&format!(
        "- **Evidence Confidence:** [{}]({})\n",
        dossier.confidence, dossier.confidence_docs
    ));
    if !dossier.rejected_prs.is_empty() {
        for rej in &dossier.rejected_prs {
            out.push_str(&format!("- **Audit Trail:** {}\n", rej.audit_message()));
        }
    }
    out.push('\n');

    out.push_str("#### Evolution Timeline\n");
    for m in &dossier.evolution {
        let pr = m
            .pr_reference
            .as_deref()
            .map(|p| format!(" ({p})"))
            .unwrap_or_default();
        let marker = if m.is_genesis { "★ Genesis:" } else { "●" };
        out.push_str(&format!(
            "- {} **{}** [`{}`]{}: {}\n",
            marker, m.date, m.short_sha, pr, m.summary
        ));
    }

    if let Some(ref diff) = dossier.genesis_diff {
        out.push_str("\n#### 🔍 Genesis Diff (Introducing Patch)\n\n```diff\n");
        out.push_str(diff);
        out.push_str("\n```\n");
    }

    out
}
