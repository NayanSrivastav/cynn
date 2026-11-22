use crate::core::LineageDossier;

/// Render the archaeological dossier as GitHub-flavored Markdown
pub fn render_markdown(dossier: &LineageDossier) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "### Lineage Report: `{}:{}`\n\n",
        dossier.file_path, dossier.line_range
    ));

    out.push_str("```\n");
    out.push_str(&dossier.code_snippet);
    out.push_str("\n```\n\n");

    // Multi-Origin Breakdown
    if dossier.is_multi_origin {
        out.push_str("#### Multi-Origin Breakdown\n");
        out.push_str("> ⚠️ **Note:** This line range contains code introduced across multiple distinct commits. See the per-slice breakdown below for true line-by-line attribution.\n\n");
        for slice in &dossier.origins {
            let status = if slice.history_resolved {
                "Resolved"
            } else {
                "Fallback Blame"
            };
            out.push_str(&format!(
                "- **Lines {}-{}** [{status}]: [`{}`] by {} ({}) — *{}*\n",
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
        out.push_str("#### Genesis (Origin Commit)\n");
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
        out.push_str("#### Multi-Origin Range\n");
        out.push_str(&format!(
            "This code range was introduced across **{} distinct commits**. See the Multi-Origin Breakdown above for per-slice provenance.\n",
            dossier.origins.len()
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
        "- **Evidence Confidence:** [{}]({})\n\n",
        dossier.confidence, dossier.confidence_docs
    ));

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

    out
}
