use crate::core::LineageDossier;
use owo_colors::OwoColorize;

/// Render the archaeological dossier in high-contrast ANSI colors for the terminal
#[allow(clippy::too_many_lines)]
pub fn render_ansi(dossier: &LineageDossier) -> String {
    let mut out = String::new();

    // Header box
    out.push_str(&format!(
        "╭─ Lineage: {}:{} {}\n",
        dossier.file_path.bold().cyan(),
        dossier.line_range.bold().yellow(),
        "─".repeat(40).dimmed()
    ));

    // Snippet preview
    for line in dossier.code_snippet.lines().take(5) {
        out.push_str(&format!("│ {} {}\n", "›".dimmed(), line.italic()));
    }
    if dossier.code_snippet.lines().count() > 5 {
        out.push_str(&format!("│ {} ... (truncated)\n", "›".dimmed()));
    }
    out.push_str(&format!("╰{}\n\n", "─".repeat(60).dimmed()));

    // Multi-Origin Dissection (if block spans multiple introducing commits)
    if dossier.is_multi_origin {
        out.push_str(&format!(
            "🧩 {}\n",
            format!(
                "Multi-Origin Range ({} distinct introducing commits):",
                dossier.origins.len()
            )
            .bold()
            .yellow()
        ));
        for slice in &dossier.origins {
            let status = if slice.is_verified {
                "[verified]".green().to_string()
            } else {
                "[fallback]".dimmed().to_string()
            };
            out.push_str(&format!(
                "   Lines {}-{}: {} {} by {} ({})\n     \"{}\"\n",
                slice.line_start,
                slice.line_end,
                slice.short_sha.yellow(),
                status,
                slice.author.green(),
                slice.date.dimmed(),
                slice.summary.dimmed()
            ));
        }
        out.push('\n');
    }

    // Genesis Block
    let genesis_title = if dossier.is_multi_origin {
        "📌 Primary Genesis Commit (Foundational Origin)"
    } else {
        "📌 Genesis (Introducing Commit)"
    };
    out.push_str(&format!("{}\n", genesis_title.bold()));
    out.push_str(&format!(
        "   Commit:    {} ({})\n",
        dossier.genesis_commit.short_sha.yellow().bold(),
        dossier.genesis_commit.author_date.dimmed()
    ));
    out.push_str(&format!(
        "   Author:    {} <{}>\n",
        dossier.genesis_commit.author_name.green(),
        dossier.genesis_commit.author_email.dimmed()
    ));

    if let Some(ref pr) = dossier.pr_reference {
        out.push_str(&format!("   Reference: {}\n", pr.bright_magenta().bold()));
    }

    out.push_str(&format!(
        "   Intent:    \"{}\"\n\n",
        dossier.intent_summary.white()
    ));

    // Evolution Timeline
    out.push_str(&format!("📜 {}\n", "Evolution Timeline".bold()));
    for m in &dossier.evolution {
        let marker = if m.is_genesis {
            "★".yellow().to_string()
        } else {
            "●".blue().to_string()
        };
        let pr_tag = m
            .pr_reference
            .as_deref()
            .map(|p| format!("({p})"))
            .unwrap_or_default();

        out.push_str(&format!(
            "   {} {}  {}  {}\n",
            marker,
            m.date.dimmed(),
            m.short_sha.yellow(),
            pr_tag.magenta()
        ));
        out.push_str(&format!("     {}\n", m.summary));
    }
    out.push('\n');

    // Evidence & Confidence
    out.push_str(&format!("🧪 {}\n", "Evidence & Confidence".bold()));
    if !dossier.related_tests.is_empty() {
        out.push_str(&format!(
            "   Related Tests: {}\n",
            dossier.related_tests.join(", ").cyan()
        ));
    }
    if let Some(ref pr) = dossier.pr_details {
        out.push_str(&format!(
            "   Context URL:   {}\n",
            pr.url.underline().blue()
        ));
        if !pr.linked_issues.is_empty() {
            out.push_str(&format!(
                "   Linked Issues: {}\n",
                pr.linked_issues.join(", ").bright_yellow()
            ));
        }
        if !pr.reviewer_highlights.is_empty() {
            out.push_str(&format!(
                "   Review Note:   {}\n",
                pr.reviewer_highlights[0].italic().dimmed()
            ));
        }
    }
    out.push_str(&format!(
        "   Evidence Items: {} structured citations\n",
        dossier.evidence.len()
    ));
    out.push_str(&format!("   Confidence:    {}\n", dossier.confidence));

    out
}
