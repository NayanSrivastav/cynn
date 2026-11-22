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

    out.push_str("#### Genesis (Origin Commit)\n");
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
        out.push_str(&format!(
            "- **{}** [`{}`]{}: {}\n",
            m.date, m.short_sha, pr, m.summary
        ));
    }

    out
}
