use std::path::{Path, PathBuf};
use std::str::FromStr;

/// A parsed target representing a file and an optional line or line range.
///
/// Supported formats:
/// - `path/to/file.rs:42` (single line)
/// - `path/to/file.rs:40-60` (line range)
/// - `path/to/file.rs` (entire file)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
}

impl Target {
    /// Create a target for an entire file
    pub fn for_file(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            start_line: None,
            end_line: None,
        }
    }

    /// Check if target specifies a specific line or range
    pub fn has_line_spec(&self) -> bool {
        self.start_line.is_some()
    }
}

impl FromStr for Target {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("Target string cannot be empty".to_string());
        }

        // Split on the last colon to separate path from line specification
        if let Some((path_str, line_spec)) = s.rsplit_once(':') {
            let path = PathBuf::from(path_str);

            if let Some((start_str, end_str)) = line_spec.split_once('-') {
                let start = start_str
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid start line number: '{start_str}'"))?;
                let end = end_str
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid end line number: '{end_str}'"))?;

                if start == 0 || end == 0 {
                    return Err("Line numbers must be 1-indexed (>= 1)".to_string());
                }
                if start > end {
                    return Err(format!("Start line ({start}) cannot exceed end line ({end})"));
                }

                Ok(Self {
                    path,
                    start_line: Some(start),
                    end_line: Some(end),
                })
            } else {
                let line = line_spec
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid line number: '{line_spec}'"))?;

                if line == 0 {
                    return Err("Line numbers must be 1-indexed (>= 1)".to_string());
                }

                Ok(Self {
                    path,
                    start_line: Some(line),
                    end_line: Some(line),
                })
            }
        } else {
            Ok(Self::for_file(Path::new(s)))
        }
    }
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
    }

    #[test]
    fn test_parse_range() {
        let t: Target = "src/billing/service.ts:10-25".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/billing/service.ts"));
        assert_eq!(t.start_line, Some(10));
        assert_eq!(t.end_line, Some(25));
    }

    #[test]
    fn test_parse_file_only() {
        let t: Target = "src/main.rs".parse().unwrap();
        assert_eq!(t.path, PathBuf::from("src/main.rs"));
        assert_eq!(t.start_line, None);
        assert_eq!(t.end_line, None);
    }
}
