use crate::core::LineageDossier;
use owo_colors::OwoColorize;

/// Render the archaeological dossier in high-contrast ANSI colors for the terminal
#[allow(clippy::too_many_lines)]
pub fn render_ansi(dossier: &LineageDossier) -> String {
    let mut out = String::new();

    // Header box and code preview
    render_header_box(dossier, &mut out);

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
        if let Some(ref assoc) = pr.association {
            out.push_str(&format!("   Association:   {}\n", assoc.note.yellow()));
        }
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
    if !dossier.rejected_prs.is_empty() {
        for rej in &dossier.rejected_prs {
            out.push_str(&format!(
                "   Audit Note:    {}\n",
                rej.audit_message().yellow()
            ));
        }
    }

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

struct BoxLine {
    visible_width: usize,
    rendered: String,
}

fn char_width(c: char) -> usize {
    if c.is_control() {
        return 0;
    }
    match c {
        '\u{1100}'..='\u{115F}'
        | '\u{2329}'..='\u{232A}'
        | '\u{2E80}'..='\u{A4CF}'
        | '\u{AC00}'..='\u{D7A3}'
        | '\u{F900}'..='\u{FAFF}'
        | '\u{FE10}'..='\u{FE19}'
        | '\u{FE30}'..='\u{FE6F}'
        | '\u{FF00}'..='\u{FF60}'
        | '\u{FFE0}'..='\u{FFE6}'
        | '\u{1F300}'..='\u{1F64F}'
        | '\u{1F680}'..='\u{1F6FF}' => 2,
        _ => 1,
    }
}

fn visible_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn expand_tabs(s: &str) -> String {
    s.replace('\t', "    ")
}

fn truncate_to_width(s: &str, max_w: usize) -> (String, usize) {
    let mut current_w = 0;
    let mut out = String::new();
    for c in s.chars() {
        let w = char_width(c);
        if current_w + w + 1 > max_w {
            out.push('…');
            current_w += 1;
            return (out, current_w);
        }
        out.push(c);
        current_w += w;
    }
    (out, current_w)
}

#[allow(clippy::too_many_lines)]
fn render_header_box(dossier: &LineageDossier, out: &mut String) {
    const MAX_BOX_WIDTH: usize = 160;

    let scope_opt = dossier
        .snippet_accordion
        .as_ref()
        .and_then(|a| a.scope.as_deref());

    // 1. Compute lines to render inside the box
    let box_lines: Vec<BoxLine> = if let Some(ref accordion) = dossier.snippet_accordion {
        let gutter_width = accordion.max_line_number.to_string().len().max(2);
        let mut lines = Vec::new();
        for line in &accordion.lines {
            let clean_content = expand_tabs(&line.content);
            let marker_vis = visible_width(&line.marker);
            let prefix_vis = gutter_width + 1 + marker_vis + 1; // "<gutter> <marker> "

            let max_content_w = MAX_BOX_WIDTH.saturating_sub(prefix_vis + 4);
            let (content_str, content_vis) = if visible_width(&clean_content) > max_content_w {
                truncate_to_width(&clean_content, max_content_w)
            } else {
                (clean_content.clone(), visible_width(&clean_content))
            };

            let total_vis = prefix_vis + content_vis;

            let rendered = if line.is_fold_marker {
                format!(
                    "{:>width$} {} {}",
                    "",
                    line.marker.yellow().dimmed(),
                    content_str.italic().yellow().dimmed(),
                    width = gutter_width
                )
            } else if line.is_target {
                let num_str = format!("{:>width$}", line.line_number, width = gutter_width);
                format!(
                    "{} {} {}",
                    num_str.bold().yellow(),
                    line.marker.bold().yellow(),
                    content_str.white()
                )
            } else {
                let num_str = format!("{:>width$}", line.line_number, width = gutter_width);
                format!(
                    "{} {} {}",
                    num_str.dimmed(),
                    line.marker.dimmed(),
                    content_str.dimmed()
                )
            };

            lines.push(BoxLine {
                visible_width: total_vis,
                rendered,
            });
        }
        lines
    } else {
        let mut lines = Vec::new();
        let max_content_w = MAX_BOX_WIDTH.saturating_sub(6); // "› " is 2, borders are 4
        for line in dossier.code_snippet.lines().take(5) {
            let clean_content = expand_tabs(line);
            let (content_str, content_vis) = if visible_width(&clean_content) > max_content_w {
                truncate_to_width(&clean_content, max_content_w)
            } else {
                (clean_content.clone(), visible_width(&clean_content))
            };
            let total_vis = 2 + content_vis;
            let rendered = format!("{} {}", "›".dimmed(), content_str.italic());
            lines.push(BoxLine {
                visible_width: total_vis,
                rendered,
            });
        }
        if dossier.code_snippet.lines().count() > 5 {
            lines.push(BoxLine {
                visible_width: 17,
                rendered: format!("{} ... (truncated)", "›".dimmed()),
            });
        }
        lines
    };

    // 2. Calculate visible width of top header prefix
    // "╭─ Lineage: <file_path>:<line_range>" (+ " ─ [ <scope> ]") + " "
    let top_prefix_vis = 3 // "╭─ "
        + 9 // "Lineage: "
        + visible_width(&dossier.file_path)
        + 1 // ":"
        + visible_width(&dossier.line_range)
        + scope_opt.map_or(0, |s| 5 + visible_width(s) + 2) // " ─ [ " + s + " ]"
        + 1; // " " before dashes

    let min_top_width = top_prefix_vis + 3; // at least 2 dashes + 1 corner "╮"

    let max_body_vis = box_lines.iter().map(|l| l.visible_width).max().unwrap_or(0);
    let min_body_width = max_body_vis + 4; // "│ " (2) + content + " │" (2)

    let env_cols = std::env::var("COLUMNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(85);

    let base_target_width = env_cols.clamp(80, 85);
    let box_width = base_target_width.max(min_top_width).max(min_body_width);

    // 3. Render Top border
    let dashes_count = box_width.saturating_sub(top_prefix_vis + 1); // 1 for "╮"
    let dashes_str = "─".repeat(dashes_count);

    if let Some(scope) = scope_opt {
        out.push_str(&format!(
            "{}{}{}{}{}{}{}{}{}{}{}\n",
            "╭─ ".dimmed(),
            "Lineage: ".bold(),
            dossier.file_path.bold().cyan(),
            ":".dimmed(),
            dossier.line_range.bold().yellow(),
            " ─ [ ".dimmed(),
            scope.bold().green(),
            " ]".dimmed(),
            " ",
            dashes_str.dimmed(),
            "╮".dimmed()
        ));
    } else {
        out.push_str(&format!(
            "{}{}{}{}{}{}{}{}\n",
            "╭─ ".dimmed(),
            "Lineage: ".bold(),
            dossier.file_path.bold().cyan(),
            ":".dimmed(),
            dossier.line_range.bold().yellow(),
            " ",
            dashes_str.dimmed(),
            "╮".dimmed()
        ));
    }

    // 4. Render Body lines
    for line in box_lines {
        let padding_count = box_width.saturating_sub(4 + line.visible_width);
        let padding_str = " ".repeat(padding_count);
        out.push_str(&format!(
            "{} {}{}{}\n",
            "│".dimmed(),
            line.rendered,
            padding_str,
            " │".dimmed()
        ));
    }

    // 5. Render Bottom border
    let bottom_dashes = "─".repeat(box_width.saturating_sub(2));
    out.push_str(&format!(
        "{}{}{}\n\n",
        "╰".dimmed(),
        bottom_dashes.dimmed(),
        "╯".dimmed()
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::accordion::{AccordionLine, AccordionSnippet};
    use crate::core::Confidence;

    fn strip_ansi(s: &str) -> String {
        let mut out = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape {
                if c == 'm' {
                    in_escape = false;
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn test_char_width_and_visible_width() {
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width('╭'), 1);
        assert_eq!(char_width('╮'), 1);
        assert_eq!(char_width('│'), 1);
        assert_eq!(char_width('▾'), 1);
        assert_eq!(char_width('…'), 1);
        assert_eq!(char_width('📌'), 2);
        assert_eq!(visible_width("hello world"), 11);
        assert_eq!(visible_width("╭─ Lineage: ─╮"), 14);
    }

    #[test]
    fn test_box_rendering_with_accordion_is_symmetrical() {
        let dossier = LineageDossier {
            file_path: "src/billing/stripe_webhook.ts".to_string(),
            line_range: "88".to_string(),
            code_snippet: String::new(),
            snippet_accordion: Some(AccordionSnippet {
                scope: Some("handle_webhook".to_string()),
                lines: vec![
                    AccordionLine {
                        line_number: 87,
                        content: "const attempt = payload.attempt;".to_string(),
                        is_target: false,
                        is_fold_marker: false,
                        marker: "│".to_string(),
                    },
                    AccordionLine {
                        line_number: 88,
                        content:
                            "if (event.id.startsWith(\"evt_test_\") && attempt > 1) { return 200; }"
                                .to_string(),
                        is_target: true,
                        is_fold_marker: false,
                        marker: "▶".to_string(),
                    },
                ],
                max_line_number: 88,
            }),
            is_multi_origin: false,
            origins: Vec::new(),
            genesis_commit: None,
            fallback_used: false,
            enrichment_status: "offline".to_string(),
            pr_reference: None,
            pr_details: None,
            intent_summary: "Test intent".to_string(),
            evolution: Vec::new(),
            related_tests: Vec::new(),
            evidence: Vec::new(),
            confidence: Confidence::High,
            confidence_docs: "https://example.com".to_string(),
            rejected_prs: Vec::new(),
            genesis_diff: None,
        };

        let rendered = render_ansi(&dossier);
        let lines: Vec<&str> = rendered.lines().take(4).collect();
        assert_eq!(lines.len(), 4);

        let top_line = strip_ansi(lines[0]);
        let body_1 = strip_ansi(lines[1]);
        let body_2 = strip_ansi(lines[2]);
        let bottom_line = strip_ansi(lines[3]);

        assert!(top_line.starts_with("╭─ Lineage: "));
        assert!(top_line.ends_with('╮'));
        assert!(body_1.starts_with("│ "));
        assert!(body_1.ends_with(" │"));
        assert!(body_2.starts_with("│ "));
        assert!(body_2.ends_with(" │"));
        assert!(bottom_line.starts_with('╰'));
        assert!(bottom_line.ends_with('╯'));

        let w_top = visible_width(&top_line);
        let w_b1 = visible_width(&body_1);
        let w_b2 = visible_width(&body_2);
        let w_bottom = visible_width(&bottom_line);

        assert_eq!(w_top, w_bottom, "Top and bottom border width mismatch");
        assert_eq!(w_top, w_b1, "Top and body line 1 width mismatch");
        assert_eq!(w_top, w_b2, "Top and body line 2 width mismatch");
    }

    #[test]
    fn test_box_rendering_with_simple_snippet_is_symmetrical() {
        let dossier = LineageDossier {
            file_path: "src/billing/stripe_webhook.ts".to_string(),
            line_range: "88".to_string(),
            code_snippet:
                "if (event.id.startsWith(\"evt_test_\") && payload.attempt > 1) { return 200; }"
                    .to_string(),
            snippet_accordion: None,
            is_multi_origin: false,
            origins: Vec::new(),
            genesis_commit: None,
            fallback_used: false,
            enrichment_status: "offline".to_string(),
            pr_reference: None,
            pr_details: None,
            intent_summary: "Test intent".to_string(),
            evolution: Vec::new(),
            related_tests: Vec::new(),
            evidence: Vec::new(),
            confidence: Confidence::High,
            confidence_docs: "https://example.com".to_string(),
            rejected_prs: Vec::new(),
            genesis_diff: None,
        };

        let rendered = render_ansi(&dossier);
        let lines: Vec<&str> = rendered.lines().take(3).collect();
        assert_eq!(lines.len(), 3);

        let top_line = strip_ansi(lines[0]);
        let body = strip_ansi(lines[1]);
        let bottom_line = strip_ansi(lines[2]);

        assert!(top_line.starts_with("╭─ Lineage: "));
        assert!(top_line.ends_with('╮'));
        assert!(body.starts_with("│ "));
        assert!(body.ends_with(" │"));
        assert!(bottom_line.starts_with('╰'));
        assert!(bottom_line.ends_with('╯'));

        let w_top = visible_width(&top_line);
        let w_body = visible_width(&body);
        let w_bottom = visible_width(&bottom_line);

        assert_eq!(w_top, w_bottom, "Top and bottom border width mismatch");
        assert_eq!(w_top, w_body, "Top and body line width mismatch");
        assert_eq!(w_top, 85, "Expected standard 85 width for stripe webhook");
    }
}
