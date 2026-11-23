use serde::{Deserialize, Serialize};

/// Represents an individual line rendered within the accordion context
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccordionLine {
    /// 1-indexed line number in the source file, or 0 if this is a virtual fold marker
    pub line_number: usize,
    /// Line text content
    pub content: String,
    /// Whether this line is within the user's targeted range or target block
    pub is_target: bool,
    /// Whether this is a folded/collapsed summary line (e.g. "▸ 8 lines collapsed ...")
    pub is_fold_marker: bool,
    /// Visual marker glyph: "│" (context), "▶" (target), "▾" (block opener), "▴" (block closer), "┆" (fold)
    pub marker: String,
}

/// Structured accordion snippet providing semantic context and collapsible blocks
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccordionSnippet {
    /// Enclosing function, class, or YAML hierarchy (e.g. "renovate-notify > script")
    pub scope: Option<String>,
    /// Ordered lines to display in the accordion
    pub lines: Vec<AccordionLine>,
    /// Maximum line number in the snippet (used for gutter alignment)
    pub max_line_number: usize,
}

impl AccordionSnippet {
    /// Build a smart accordion snippet from the file lines and target range
    #[allow(clippy::too_many_lines)]
    pub fn build(file_lines: &[String], target_start: usize, target_end: usize) -> Self {
        if file_lines.is_empty() {
            return Self {
                scope: None,
                lines: Vec::new(),
                max_line_number: 0,
            };
        }

        let total_lines = file_lines.len();
        let start = target_start.clamp(1, total_lines);
        let end = target_end.clamp(start, total_lines);

        let scope = detect_scope(file_lines, start);

        // Check if the target line(s) open an unclosed block (bracket, brace, paren, or YAML/Python colon)
        let block_delimiter_end = if start == end {
            find_block_end(file_lines, start)
        } else {
            None
        };

        let mut lines = Vec::new();
        let mut max_line_number = end;

        if let Some(block_end) = block_delimiter_end {
            // Target is an opening delimiter that spans to `block_end`
            let block_len = block_end.saturating_sub(start) + 1;
            max_line_number = max_line_number.max(block_end);

            // 1. Context lines before block (up to 2 lines)
            let context_before_start = start.saturating_sub(2).max(1);
            for l in context_before_start..start {
                lines.push(AccordionLine {
                    line_number: l,
                    content: file_lines[l - 1].clone(),
                    is_target: false,
                    is_fold_marker: false,
                    marker: "│".to_string(),
                });
            }

            if block_len <= 5 {
                // Small block: render all lines without folding
                for l in start..=block_end {
                    let marker = if l == start {
                        "▾".to_string()
                    } else if l == block_end {
                        "▴".to_string()
                    } else {
                        "│".to_string()
                    };
                    lines.push(AccordionLine {
                        line_number: l,
                        content: file_lines[l - 1].clone(),
                        is_target: true,
                        is_fold_marker: false,
                        marker,
                    });
                }
            } else {
                // Large block: fold the middle lines
                // Opener line
                lines.push(AccordionLine {
                    line_number: start,
                    content: file_lines[start - 1].clone(),
                    is_target: true,
                    is_fold_marker: false,
                    marker: "▾".to_string(),
                });

                // Fold marker
                let folded_count = block_len.saturating_sub(2);
                let indent = get_indent_string(&file_lines[start - 1]);
                lines.push(AccordionLine {
                    line_number: 0,
                    content: format!("{indent}  ▸ {folded_count} lines collapsed ..."),
                    is_target: true,
                    is_fold_marker: true,
                    marker: "┆".to_string(),
                });

                // Closer line
                lines.push(AccordionLine {
                    line_number: block_end,
                    content: file_lines[block_end - 1].clone(),
                    is_target: true,
                    is_fold_marker: false,
                    marker: "▴".to_string(),
                });
            }

            // Context lines after block (up to 2 lines)
            let context_after_end = (block_end + 2).min(total_lines);
            max_line_number = max_line_number.max(context_after_end);
            for l in (block_end + 1)..=context_after_end {
                lines.push(AccordionLine {
                    line_number: l,
                    content: file_lines[l - 1].clone(),
                    is_target: false,
                    is_fold_marker: false,
                    marker: "│".to_string(),
                });
            }
        } else {
            // Standard target range (or line without delimiter)
            let context_before_start = start.saturating_sub(2).max(1);
            let context_after_end = (end + 2).min(total_lines);
            max_line_number = max_line_number.max(context_after_end);

            // Context lines before
            for l in context_before_start..start {
                lines.push(AccordionLine {
                    line_number: l,
                    content: file_lines[l - 1].clone(),
                    is_target: false,
                    is_fold_marker: false,
                    marker: "│".to_string(),
                });
            }

            // Target lines
            let target_len = end.saturating_sub(start) + 1;
            if target_len > 8 {
                // Fold long ranges
                for l in start..start + 3 {
                    lines.push(AccordionLine {
                        line_number: l,
                        content: file_lines[l - 1].clone(),
                        is_target: true,
                        is_fold_marker: false,
                        marker: "▶".to_string(),
                    });
                }

                let folded_count = target_len.saturating_sub(5);
                let indent = get_indent_string(&file_lines[start - 1]);
                lines.push(AccordionLine {
                    line_number: 0,
                    content: format!("{indent}  ▸ {folded_count} lines collapsed ..."),
                    is_target: true,
                    is_fold_marker: true,
                    marker: "┆".to_string(),
                });

                for l in (end - 1)..=end {
                    lines.push(AccordionLine {
                        line_number: l,
                        content: file_lines[l - 1].clone(),
                        is_target: true,
                        is_fold_marker: false,
                        marker: "▶".to_string(),
                    });
                }
            } else {
                for l in start..=end {
                    lines.push(AccordionLine {
                        line_number: l,
                        content: file_lines[l - 1].clone(),
                        is_target: true,
                        is_fold_marker: false,
                        marker: "▶".to_string(),
                    });
                }
            }

            // Context lines after
            for l in (end + 1)..=context_after_end {
                lines.push(AccordionLine {
                    line_number: l,
                    content: file_lines[l - 1].clone(),
                    is_target: false,
                    is_fold_marker: false,
                    marker: "│".to_string(),
                });
            }
        }

        Self {
            scope,
            lines,
            max_line_number,
        }
    }
}

/// Detect the enclosing function, class, or YAML hierarchy for a target line
pub fn detect_scope(file_lines: &[String], target_line: usize) -> Option<String> {
    if file_lines.is_empty() || target_line == 0 {
        return None;
    }

    let target_idx = target_line.min(file_lines.len()) - 1;
    let target_indent = get_indent_level(&file_lines[target_idx]);

    // Check for YAML/Configuration hierarchy by tracing lower-indentation lines backwards
    let mut yaml_keys = Vec::new();
    let mut current_indent = target_indent;

    let scan_limit = target_idx.saturating_sub(60);
    for idx in (scan_limit..target_idx).rev() {
        let line = &file_lines[idx];
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = get_indent_level(line);
        if indent < current_indent {
            if let Some(key) = extract_yaml_key(trimmed) {
                yaml_keys.push(key);
                current_indent = indent;
                if indent == 0 {
                    break;
                }
            }
        }
    }

    if !yaml_keys.is_empty() {
        yaml_keys.reverse();
        return Some(yaml_keys.join(" > "));
    }

    // Check if the target line itself is a definition (function, class, struct, etc.)
    let target_trimmed = file_lines[target_idx].trim();
    if let Some(signature) = extract_code_scope(target_trimmed) {
        return Some(signature);
    }

    // Check for enclosing code constructs (functions, classes, methods) with lower indentation
    if target_indent > 0 {
        for idx in (scan_limit..target_idx).rev() {
            let line = &file_lines[idx];
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
                continue;
            }

            let indent = get_indent_level(line);
            if indent < target_indent {
                if let Some(signature) = extract_code_scope(trimmed) {
                    return Some(signature);
                }
            }
        }
    }

    None
}

/// Extract clean YAML key name from a line (e.g. "renovate-notify:" -> "renovate-notify")
fn extract_yaml_key(trimmed: &str) -> Option<String> {
    let clean = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim();
    if let Some((key, _)) = clean.split_once(':') {
        let key = key.trim().trim_matches(|c| c == '\'' || c == '"');
        if !key.is_empty() && !key.contains(' ') {
            return Some(key.to_string());
        }
    }
    None
}

/// Extract function/class/struct scope from a code line
fn extract_code_scope(trimmed: &str) -> Option<String> {
    let patterns = [
        "pub async fn ",
        "pub fn ",
        "async fn ",
        "fn ",
        "pub struct ",
        "struct ",
        "pub enum ",
        "enum ",
        "pub trait ",
        "trait ",
        "impl ",
        "export async function ",
        "export function ",
        "export class ",
        "export const ",
        "export interface ",
        "export type ",
        "async function ",
        "function ",
        "def ",
        "class ",
        "interface ",
        "type ",
        "func ",
    ];

    for pat in patterns {
        if trimmed.starts_with(pat) {
            let rest = &trimmed[pat.trim_start().len()..];
            let name_part = if let Some(idx) = rest.find('(') {
                &rest[..idx]
            } else if let Some(idx) = rest.find('{') {
                &rest[..idx]
            } else if let Some(idx) = rest.find(':') {
                &rest[..idx]
            } else {
                rest
            };
            let cleaned = name_part.trim();
            if !cleaned.is_empty() {
                return Some(format!("{}{}", pat.trim_end(), format_scope_name(cleaned)));
            }
        }
    }

    None
}

fn format_scope_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!(" {trimmed}")
    }
}

/// Check if a line opens a block (square bracket, curly brace, paren, or colon) and find its closing line
fn find_block_end(file_lines: &[String], start_line: usize) -> Option<usize> {
    if start_line == 0 || start_line > file_lines.len() {
        return None;
    }

    let line = &file_lines[start_line - 1];
    let trimmed = line.trim();

    if trimmed.ends_with('[') {
        find_matching_delimiter(file_lines, start_line - 1, '[', ']')
    } else if trimmed.ends_with('{') {
        find_matching_delimiter(file_lines, start_line - 1, '{', '}')
    } else if trimmed.ends_with('(') {
        find_matching_delimiter(file_lines, start_line - 1, '(', ')')
    } else if trimmed.ends_with(':') {
        find_indentation_block_end(file_lines, start_line - 1)
    } else {
        None
    }
}

/// Find matching closing delimiter, taking strings and escapes into account
fn find_matching_delimiter(
    lines: &[String],
    start_line_idx: usize,
    open_char: char,
    close_char: char,
) -> Option<usize> {
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = '"';
    let mut escaped = false;

    for (idx, line) in lines.iter().enumerate().skip(start_line_idx) {
        for ch in line.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if in_string {
                if ch == string_char {
                    in_string = false;
                }
                continue;
            }
            if ch == '"' || ch == '\'' {
                in_string = true;
                string_char = ch;
                continue;
            }
            if ch == open_char {
                depth += 1;
            } else if ch == close_char {
                depth -= 1;
                if depth == 0 {
                    return Some(idx + 1); // 1-indexed
                }
            }
        }
    }

    None
}

/// Find the end of an indentation-based block (Python/YAML)
fn find_indentation_block_end(lines: &[String], start_line_idx: usize) -> Option<usize> {
    if start_line_idx >= lines.len() {
        return None;
    }

    let start_indent = get_indent_level(&lines[start_line_idx]);
    let mut last_content_line = start_line_idx + 1;
    let mut found_inner = false;

    for (idx, line) in lines.iter().enumerate().skip(start_line_idx + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = get_indent_level(line);
        if indent > start_indent {
            found_inner = true;
            last_content_line = idx + 1;
        } else if found_inner {
            return Some(last_content_line);
        } else {
            return None;
        }
    }

    if found_inner {
        Some(last_content_line)
    } else {
        None
    }
}

fn get_indent_level(line: &str) -> usize {
    line.chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum()
}

fn get_indent_string(line: &str) -> String {
    line.chars().take_while(|c| c.is_whitespace()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accordion_build_unclosed_bracket_folding() {
        let file_lines = vec![
            "job:".to_string(),
            "  script:".to_string(),
            "    blocks: [".to_string(),
            "      1,".to_string(),
            "      2,".to_string(),
            "      3,".to_string(),
            "      4,".to_string(),
            "      5,".to_string(),
            "      6,".to_string(),
            "      7,".to_string(),
            "      8,".to_string(),
            "    ]".to_string(),
            "  after_script:".to_string(),
        ];

        let accordion = AccordionSnippet::build(&file_lines, 3, 3);
        assert_eq!(accordion.scope, Some("job > script".to_string()));

        // Should have:
        // context line 1 (job:)
        // context line 2 (  script:)
        // opener line 3 (    blocks: [) marker ▾
        // fold marker (▸ 8 lines collapsed ...) marker ┆
        // closer line 12 (    ]) marker ▴
        // context line 13 (  after_script:) marker │
        let markers: Vec<&str> = accordion.lines.iter().map(|l| l.marker.as_str()).collect();
        assert_eq!(markers, vec!["│", "│", "▾", "┆", "▴", "│"]);
        assert_eq!(accordion.lines[2].line_number, 3);
        assert_eq!(accordion.lines[4].line_number, 12);
    }

    #[test]
    fn test_accordion_build_normal_line_context() {
        let file_lines = vec![
            "export function refund(amount: number) {".to_string(),
            "    if (amount > 1000) return false;".to_string(),
            "    return true;".to_string(),
            "}".to_string(),
        ];

        let accordion = AccordionSnippet::build(&file_lines, 2, 2);
        assert_eq!(accordion.scope, Some("export function refund".to_string()));
        assert_eq!(accordion.lines.len(), 4);
        assert_eq!(accordion.lines[0].marker, "│");
        assert_eq!(accordion.lines[1].marker, "▶");
        assert_eq!(accordion.lines[1].line_number, 2);
        assert_eq!(accordion.lines[2].marker, "│");
        assert_eq!(accordion.lines[3].marker, "│");
    }

    #[test]
    fn test_accordion_empty_file_safe() {
        let accordion = AccordionSnippet::build(&[], 1, 1);
        assert_eq!(accordion.lines.len(), 0);
        assert_eq!(accordion.scope, None);
    }
}
