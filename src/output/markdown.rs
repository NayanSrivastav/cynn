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
    if dossier.origins.len() > 1 {
        out.push_str("#### Multi-Origin Breakdown\n");
        out.push_str("This line range contains code introduced in distinct commits:\n\n");
        for slice in &dossier.origins {
            out.push_str(&format!(
                "- **Lines {}-{}**: [`{}`] by {} ({}) — *{}*\n",
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

    out.push_str("#### Genesis (Primary Origin Commit)\n");
    out.push_str(&format!(
        "- **Commit:** `{}` ({})\n",
        dossier.genesis_commit.short_sha, dossier.genesis_commit.author_date
    ));
    out.push_str(&format!(
        "- **Author:** {} `<{}>`\n",
        dossier.genesis_commit.author_name, dossier.genesis_commit.author_email
    ));
    if let Some(ref pr) = dossier.pr_details {
        out.push_str(&format!("- **Pull Request:** [{}]({})\n", pr.title, pr.url));
        if !pr.linked_issues.is_empty() {
            out.push_str(&format!(
                "- **Linked Issues:** {}\n",
                pr.linked_issues.join(", ")
            ));
        }
        if !pr.reviewer_highlights.is_empty() {
            out.push_str(&format!(
                "- **Reviewer Note:** {}\n",
                pr.reviewer_highlights[0]
            ));
        }
    } else if let Some(ref pr) = dossier.pr_reference {
        out.push_str(&format!("- **PR Reference:** {pr}\n"));
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
        "- **Evidence Confidence:** {}\n\n",
        dossier.confidence
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
