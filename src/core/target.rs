use std::path::{Path, PathBuf};
use std::str::FromStr;

/// A parsed target representing a file and an optional line, line range, or function/symbol.
///
/// Supported formats:
/// - `path/to/file.rs:42` (single line)
/// - `path/to/file.rs:40-60` (line range)
/// - `path/to/file.rs:function_name` (named function / method)
/// - `path/to/file.rs::function_name` (C++/Rust double-colon syntax)
/// - `:function_name:path/to/file.rs` (git log -L syntax)
/// - `path/to/file.rs` (entire file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub function_name: Option<String>,
}

impl Target {
    /// Create a target for an entire file
    pub fn for_file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            start_line: None,
            end_line: None,
            function_name: None,
        }
    }

    /// Create a target for a specific line or range
    pub fn for_range(path: impl Into<PathBuf>, start: usize, end: usize) -> Self {
        Self {
            path: path.into(),
            start_line: Some(start),
            end_line: Some(end),
            function_name: None,
        }
    }

    /// Create a target for a named function or symbol
    pub fn for_function(path: impl Into<PathBuf>, func: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            start_line: None,
            end_line: None,
            function_name: Some(func.into()),
        }
    }

    /// Check if target specifies a specific line, range, or function
    pub fn has_line_spec(&self) -> bool {
        self.start_line.is_some() || self.function_name.is_some()
    }
}

impl FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Target string cannot be empty".to_string());
        }

        // Support git log -L style: :func:path/to/file.rs
        if let Some(rest) = s.strip_prefix(':') {
            if let Some((func, path_str)) = rest.split_once(':') {
                let func_trimmed = func.trim();
                let path_trimmed = path_str.trim();
                if !func_trimmed.is_empty() && !path_trimmed.is_empty() {
                    return Ok(Self {
                        path: PathBuf::from(path_trimmed),
                        start_line: None,
                        end_line: None,
                        function_name: Some(func_trimmed.to_string()),
                    });
                }
            }
        }

        // Support C++/Rust double-colon style: path/to/file.rs::func
        if let Some((path_str, func)) = s.split_once("::") {
            let func_trimmed = func.trim();
            let path_trimmed = path_str.trim();
            if !func_trimmed.is_empty() && !path_trimmed.is_empty() {
                return Ok(Self {
                    path: PathBuf::from(path_trimmed),
                    start_line: None,
                    end_line: None,
                    function_name: Some(func_trimmed.to_string()),
                });
            }
        }

        // Split on the last colon to separate path from line specification or function
        if let Some((path_str, spec)) = s.rsplit_once(':') {
            let path = PathBuf::from(path_str);
            let spec_trimmed = spec.trim();

            if let Some((start_str, end_str)) = spec_trimmed.split_once('-') {
                if let (Ok(start), Ok(end)) = (start_str.parse::<usize>(), end_str.parse::<usize>())
                {
                    if start == 0 || end == 0 {
                        return Err("Line numbers must be 1-indexed (>= 1)".to_string());
                    }
                    if start > end {
                        return Err(format!(
                            "Start line ({start}) cannot exceed end line ({end})"
                        ));
                    }

                    return Ok(Self {
                        path,
                        start_line: Some(start),
                        end_line: Some(end),
                        function_name: None,
                    });
                }
            }

            if let Ok(line) = spec_trimmed.parse::<usize>() {
                if line == 0 {
                    return Err("Line numbers must be 1-indexed (>= 1)".to_string());
                }

                return Ok(Self {
                    path,
                    start_line: Some(line),
                    end_line: Some(line),
                    function_name: None,
                });
            }

            // If not numeric, treat as function or symbol name
            if !spec_trimmed.is_empty() {
                return Ok(Self {
                    path,
                    start_line: None,
                    end_line: None,
                    function_name: Some(spec_trimmed.to_string()),
                });
            }

            Ok(Self::for_file(path))
        } else {
            Ok(Self::for_file(Path::new(s)))
        }
    }
}

/// Helper to determine if a string contains a target identifier as a discrete token / word
fn contains_identifier_token(line: &str, ident: &str) -> bool {
    let mut search_idx = 0;
    while let Some(pos) = line[search_idx..].find(ident) {
        let actual_pos = search_idx + pos;
        let before_ok = if actual_pos == 0 {
            true
        } else {
            let prev_char = line[..actual_pos].chars().last().unwrap();
            !prev_char.is_alphanumeric() && prev_char != '_'
        };

        let end_pos = actual_pos + ident.len();
        let after_ok = if end_pos >= line.len() {
            true
        } else {
            let next_char = line[end_pos..].chars().next().unwrap();
            !next_char.is_alphanumeric() && next_char != '_'
        };

        if before_ok && after_ok {
            return true;
        }
        search_idx = actual_pos + ident.len();
    }
    false
}

/// Helper to score a candidate line as a function definition
fn score_function_candidate(trimmed: &str, ident: &str) -> usize {
    if !contains_identifier_token(trimmed, ident) {
        return 0;
    }

    // Skip comments
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with("/*")
    {
        return 0;
    }

    let kw_patterns = [
        format!("fn {ident}"),
        format!("def {ident}"),
        format!("func {ident}"),
        format!("function {ident}"),
        format!("fun {ident}"),
    ];

    for pat in &kw_patterns {
        if trimmed.contains(pat) {
            return 10;
        }
    }

    // Go receiver method: func (...) ident(
    if trimmed.starts_with("func ") && trimmed.contains(&format!(" {ident}(")) {
        return 10;
    }

    // JS/TS assigned arrow/function: ident = or ident: function
    if trimmed.contains(&format!("{ident} =")) || trimmed.contains(&format!("{ident}: function")) {
        return 8;
    }

    // Class/interface method or C-style function: ident(...)
    if trimmed.contains(&format!("{ident}(")) || trimmed.contains(&format!("{ident} (")) {
        return 6;
    }

    1
}

/// Helper to count leading indentation level (treating tab as 4 spaces)
fn get_indent_level(line: &str) -> usize {
    let mut indent = 0;
    for ch in line.chars() {
        if ch == ' ' {
            indent += 1;
        } else if ch == '\t' {
            indent += 4;
        } else {
            break;
        }
    }
    indent
}

/// Resolves a function or symbol name to a (`start_line`, `end_line`) 1-indexed range within `file_lines`.
#[allow(clippy::too_many_lines)]
pub fn resolve_function_range(
    file_lines: &[String],
    function_name: &str,
) -> Result<(usize, usize), String> {
    if file_lines.is_empty() {
        return Err("File is empty; cannot resolve function".to_string());
    }

    let target_symbol = function_name.trim();
    if target_symbol.is_empty() {
        return Err("Function name cannot be empty".to_string());
    }

    let mut best_match: Option<(usize, usize)> = None; // (line_idx, score)

    for (idx, line) in file_lines.iter().enumerate() {
        let trimmed = line.trim();
        let score = score_function_candidate(trimmed, target_symbol);
        if score > 0 {
            if let Some((_, best_score)) = best_match {
                if score > best_score {
                    best_match = Some((idx, score));
                }
            } else {
                best_match = Some((idx, score));
            }
        }
    }

    let Some((match_idx, _)) = best_match else {
        return Err(format!(
            "Could not locate definition for function or symbol '{target_symbol}'"
        ));
    };

    let start_line = match_idx + 1;

    // Check for brace-delimited block: scan from match_idx forward for opening '{'
    let brace_start_idx = file_lines
        .iter()
        .enumerate()
        .skip(match_idx)
        .take(10)
        .find(|(_, l)| l.contains('{'))
        .map(|(idx, _)| idx);

    if let Some(b_idx) = brace_start_idx {
        let mut depth = 0;
        let mut in_string = false;
        let mut string_char = '"';
        let mut escaped = false;

        for (idx, line) in file_lines.iter().enumerate().skip(b_idx) {
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
                if ch == '"' || ch == '\'' || ch == '`' {
                    in_string = true;
                    string_char = ch;
                    continue;
                }
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        return Ok((start_line, idx + 1));
                    }
                }
            }
        }
    }

    // Indentation-based delimiter (Python, YAML)
    let def_indent = get_indent_level(&file_lines[match_idx]);
    let mut end_line = start_line;
    let mut in_body = false;

    for (idx, line) in file_lines.iter().enumerate().skip(match_idx + 1) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = get_indent_level(line);
        if indent > def_indent {
            in_body = true;
            end_line = idx + 1;
        } else if in_body && indent <= def_indent {
            break;
        }
    }

    Ok((start_line, end_line))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_line() {
        let t: Target = "src/payment.rs:42".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/payment.rs"));
        assert_eq!(t.start_line, Some(42));
        assert_eq!(t.end_line, Some(42));
        assert_eq!(t.function_name, None);
    }

    #[test]
    fn test_parse_range() {
        let t: Target = "src/billing/service.ts:10-25".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/billing/service.ts"));
        assert_eq!(t.start_line, Some(10));
        assert_eq!(t.end_line, Some(25));
        assert_eq!(t.function_name, None);
    }

    #[test]
    fn test_parse_function_name() {
        let t: Target = "src/payment.rs:processWebhook".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/payment.rs"));
        assert_eq!(t.start_line, None);
        assert_eq!(t.end_line, None);
        assert_eq!(t.function_name, Some("processWebhook".to_string()));
    }

    #[test]
    fn test_parse_double_colon_function() {
        let t: Target = "src/payment.rs::processWebhook".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/payment.rs"));
        assert_eq!(t.start_line, None);
        assert_eq!(t.end_line, None);
        assert_eq!(t.function_name, Some("processWebhook".to_string()));
    }

    #[test]
    fn test_parse_git_log_l_function_syntax() {
        let t: Target = ":processWebhook:src/payment.rs".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/payment.rs"));
        assert_eq!(t.start_line, None);
        assert_eq!(t.end_line, None);
        assert_eq!(t.function_name, Some("processWebhook".to_string()));
    }

    #[test]
    fn test_parse_file_only() {
        let t: Target = "src/main.rs".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/main.rs"));
        assert_eq!(t.start_line, None);
        assert_eq!(t.end_line, None);
        assert_eq!(t.function_name, None);
    }

    #[test]
    fn test_resolve_rust_function() {
        let lines = vec![
            "use std::io;".to_string(),
            String::new(),
            "pub fn calculate(x: i32) -> i32 {".to_string(),
            "    if x > 0 {".to_string(),
            "        x * 2".to_string(),
            "    } else {".to_string(),
            "        0".to_string(),
            "    }".to_string(),
            "}".to_string(),
            String::new(),
        ];
        let (start, end) = resolve_function_range(&lines, "calculate").unwrap();
        assert_eq!(start, 3);
        assert_eq!(end, 9);
    }

    #[test]
    fn test_resolve_python_function() {
        let lines = vec![
            "import os".to_string(),
            String::new(),
            "def handle_request(event):".to_string(),
            "    payload = event.get('body')".to_string(),
            "    if not payload:".to_string(),
            "        return None".to_string(),
            "    return payload".to_string(),
            String::new(),
            "def other_func():".to_string(),
            "    pass".to_string(),
        ];
        let (start, end) = resolve_function_range(&lines, "handle_request").unwrap();
        assert_eq!(start, 3);
        assert_eq!(end, 7);
    }

    #[test]
    fn test_resolve_java_allman_style() {
        let lines = vec![
            "public class Service {".to_string(),
            "    public void executeJob()".to_string(),
            "    {".to_string(),
            "        doWork();".to_string(),
            "    }".to_string(),
            "}".to_string(),
        ];
        let (start, end) = resolve_function_range(&lines, "executeJob").unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 5);
    }
}
