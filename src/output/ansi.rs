use crate::core::LineageDossier;
use owo_colors::OwoColorize;

/// Render the archaeological dossier in high-contrast ANSI colors for the terminal
#[allow(clippy::too_many_lines)]
pub fn render_ansi(dossier: &LineageDossier) -> String {
    let mut out = String::new();

    // Header box and code preview
    if let Some(ref accordion) = dossier.snippet_accordion {
        if let Some(ref scope) = accordion.scope {
            out.push_str(&format!(
                "╭─ Lineage: {}:{} ─ [ {} ] {}\n",
                dossier.file_path.bold().cyan(),
                dossier.line_range.bold().yellow(),
                scope.bold().green(),
                "─".repeat(20).dimmed()
            ));
        } else {
            out.push_str(&format!(
                "╭─ Lineage: {}:{} {}\n",
                dossier.file_path.bold().cyan(),
                dossier.line_range.bold().yellow(),
                "─".repeat(40).dimmed()
            ));
        }

        let gutter_width = accordion.max_line_number.to_string().len().max(2);
        for line in &accordion.lines {
            if line.is_fold_marker {
                out.push_str(&format!(
                    "│ {:>width$} {} {}\n",
                    "",
                    line.marker.yellow().dimmed(),
                    line.content.italic().yellow().dimmed(),
                    width = gutter_width
                ));
            } else if line.is_target {
                let num_str = format!("{:>width$}", line.line_number, width = gutter_width);
                out.push_str(&format!(
                    "│ {} {} {}\n",
                    num_str.bold().yellow(),
                    line.marker.bold().yellow(),
                    line.content.white()
                ));
            } else {
                let num_str = format!("{:>width$}", line.line_number, width = gutter_width);
                out.push_str(&format!(
                    "│ {} {} {}\n",
                    num_str.dimmed(),
                    line.marker.dimmed(),
                    line.content.dimmed()
                ));
            }
        }
        out.push_str(&format!("╰{}\n\n", "─".repeat(60).dimmed()));
    } else {
        out.push_str(&format!(
            "╭─ Lineage: {}:{} {}\n",
            dossier.file_path.bold().cyan(),
            dossier.line_range.bold().yellow(),
            "─".repeat(40).dimmed()
        ));

        for line in dossier.code_snippet.lines().take(5) {
            out.push_str(&format!("│ {} {}\n", "›".dimmed(), line.italic()));
        }
        if dossier.code_snippet.lines().count() > 5 {
            out.push_str(&format!("│ {} ... (truncated)\n", "›".dimmed()));
        }
        out.push_str(&format!("╰{}\n\n", "─".repeat(60).dimmed()));
    }

    // Multi-Origin Dissection (if block spans multiple introducing commits)
    if dossier.is_multi_origin {
        let distinct_count = dossier
            .origins
            .iter()
            .map(|s| &s.genesis_sha)
            .collect::<std::collections::HashSet<_>>()
            .len();

        let commit_str = if distinct_count == 1 {
            "1 distinct introducing commit".to_string()
        } else {
            format!("{distinct_count} distinct introducing commits")
        };

        let slice_str = if dossier.origins.len() == 1 {
            "1 origin slice".to_string()
        } else {
            format!("{} origin slices", dossier.origins.len())
        };

        out.push_str(&format!(
            "🧩 {}\n",
            format!("Multi-Origin Range ({commit_str} across {slice_str}):")
                .bold()
                .yellow()
        ));
        for slice in &dossier.origins {
            let status = if slice.fallback_used {
                "[unverified-fallback]".red().to_string()
            } else if slice.history_resolved {
                "[resolved]".green().to_string()
            } else {
                "[blame-fallback]".dimmed().to_string()
            };
            out.push_str(&format!(
                "   Lines {}-{}: {} {} by {} ({})\n",
                slice.line_start,
                slice.line_end,
                slice.short_sha.yellow(),
                status,
                slice.author.green(),
                slice.date.dimmed()
            ));
            if !slice.blame_commits.is_empty()
                && (slice.blame_commits.len() > 1 || slice.blame_commits[0] != slice.genesis_sha)
            {
                let short_blames: Vec<&str> = slice
                    .blame_commits
                    .iter()
                    .map(|sha| if sha.len() >= 7 { &sha[..7] } else { sha })
                    .collect();
                out.push_str(&format!(
                    "     Last modified in HEAD blame: {}\n",
                    short_blames.join(", ").dimmed()
                ));
            }
            out.push_str(&format!("     \"{}\"\n", slice.summary.dimmed()));
        }
        out.push('\n');
    }

    // Genesis Block
    if let Some(ref genesis) = dossier.genesis_commit {
        let fallback_tag = if dossier.fallback_used {
            format!(" {}", "(Fallback - Unverified DAG)".red())
        } else {
            String::new()
        };
        out.push_str(&format!(
            "{}{}\n",
            "📌 Genesis (Introducing Commit)".bold(),
            fallback_tag
        ));
        out.push_str(&format!(
            "   Commit:    {} ({})\n",
            genesis.short_sha.yellow().bold(),
            genesis.author_date.dimmed()
        ));
        out.push_str(&format!(
            "   Author:    {} <{}>\n",
            genesis.author_name.green(),
            genesis.author_email.dimmed()
        ));

        if let Some(ref pr) = dossier.pr_reference {
            out.push_str(&format!("   Reference: {}\n", pr.bright_magenta().bold()));
        }

        out.push_str(&format!(
            "   Intent:    \"{}\"\n\n",
            dossier.intent_summary.white()
        ));
    } else {
        let distinct_count = dossier
            .origins
            .iter()
            .map(|s| &s.genesis_sha)
            .collect::<std::collections::HashSet<_>>()
            .len();
        let commit_str = if distinct_count == 1 {
            "1 distinct introducing commit".to_string()
        } else {
            format!("{distinct_count} distinct introducing commits")
        };
        out.push_str(&format!(
            "{}\n   Range is a composite of {commit_str}.\n   Review the Origin Slices above for authoritative per-line attribution.\n   Intent: \"{}\"\n\n",
            "📌 Multi-Origin Composition".bold().yellow(),
            dossier.intent_summary.white()
        ));
    }

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
            let h = &pr.reviewer_highlights[0];
            let state = h.review_state.as_deref().unwrap_or("note");
            out.push_str(&format!(
                "   Review Note:   {} ({}): {}\n",
                h.author.green(),
                state.dimmed(),
                h.body.italic().dimmed()
            ));
        }
    }
    out.push_str(&format!(
        "   Evidence Items: {} structured citations\n",
        dossier.evidence.len()
    ));
    out.push_str(&format!("   Confidence:    {}\n", dossier.confidence));
    out.push_str(&format!(
        "   Confidence Docs: {}\n",
        dossier.confidence_docs.underline().blue()
    ));

    // Genesis Diff (when requested via --diff)
    if let Some(ref diff) = dossier.genesis_diff {
        out.push('\n');
        out.push_str(&format!(
            "🔍 {}\n",
            "Genesis Diff (Introducing Patch)".bold()
        ));
        for line in diff.lines().take(120) {
            if line.starts_with('+') && !line.starts_with("+++") {
                out.push_str(&format!("   {}\n", line.green()));
            } else if line.starts_with('-') && !line.starts_with("---") {
                out.push_str(&format!("   {}\n", line.red()));
            } else if line.starts_with('@') {
                out.push_str(&format!("   {}\n", line.cyan()));
            } else {
                out.push_str(&format!("   {}\n", line.dimmed()));
            }
        }
        let total_lines = diff.lines().count();
        if total_lines > 120 {
            out.push_str(&format!(
                "   {}\n",
                format!(
                    "... [{} lines truncated, use git show to inspect full diff]",
                    total_lines - 120
                )
                .dimmed()
            ));
        }
    }

    out
}
