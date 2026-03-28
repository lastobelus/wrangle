//! Output filtering utilities
//!
//! These utilities are reserved for output processing and metrics extraction.

#![allow(dead_code)] // Reserved API: output filtering will be used when metrics reporting is added

use regex::Regex;

/// Output filter for cleaning backend output
pub struct OutputFilter {
    patterns: Vec<Regex>,
}

impl OutputFilter {
    /// Create a new output filter with default patterns
    pub fn new() -> Self {
        let patterns = vec![
            // ANSI escape codes
            Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]").unwrap(),
            // Control characters
            Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f]").unwrap(),
        ];

        Self { patterns }
    }

    /// Filter output string
    pub fn filter(&self, input: &str) -> String {
        let mut result = input.to_string();
        for pattern in &self.patterns {
            result = pattern.replace_all(&result, "").to_string();
        }
        result
    }
}

impl Default for OutputFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Sanitize output for JSON embedding
pub fn sanitize_for_json(input: &str) -> String {
    input.replace(['\x00', '\x08', '\x0c'], "")
}

/// Extract test coverage from output
pub fn extract_coverage(output: &str) -> Option<f64> {
    // Look for common coverage patterns
    let patterns = [
        r"Coverage:\s*(\d+(?:\.\d+)?)\s*%",
        r"(\d+(?:\.\d+)?)\s*%\s*(?:coverage|covered)",
        r"All files\s*\|\s*(\d+(?:\.\d+)?)",
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern)
            && let Some(caps) = re.captures(output)
            && let Some(m) = caps.get(1)
            && let Ok(val) = m.as_str().parse::<f64>()
        {
            return Some(val);
        }
    }

    None
}

/// Extract files changed count from output
pub fn extract_files_changed(output: &str) -> Option<usize> {
    let patterns = [
        r"(\d+)\s*files?\s*changed",
        r"Changed\s*(\d+)\s*files?",
        r"Modified:\s*(\d+)",
    ];

    for pattern in patterns {
        if let Ok(re) = Regex::new(pattern)
            && let Some(caps) = re.captures(output)
            && let Some(m) = caps.get(1)
            && let Ok(val) = m.as_str().parse::<usize>()
        {
            return Some(val);
        }
    }

    None
}

/// Extract test results from output
pub fn extract_test_results(output: &str) -> Option<(usize, usize, usize)> {
    // Pattern: X passed, Y failed, Z skipped
    if let Ok(re) = Regex::new(r"(\d+)\s*passed.*?(\d+)\s*failed.*?(\d+)\s*skipped")
        && let Some(caps) = re.captures(output)
    {
        let passed = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let failed = caps
            .get(2)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        let skipped = caps
            .get(3)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        return Some((passed, failed, skipped));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_filter() {
        let filter = OutputFilter::new();
        let input = "Hello \x1b[32mWorld\x1b[0m";
        let output = filter.filter(input);
        assert_eq!(output, "Hello World");
    }

    #[test]
    fn test_extract_coverage() {
        assert_eq!(extract_coverage("Coverage: 85.5%"), Some(85.5));
        assert_eq!(extract_coverage("95% coverage"), Some(95.0));
        assert_eq!(extract_coverage("no coverage info"), None);
    }

    #[test]
    fn test_extract_files_changed() {
        assert_eq!(extract_files_changed("3 files changed"), Some(3));
        assert_eq!(extract_files_changed("Changed 5 files"), Some(5));
        assert_eq!(extract_files_changed("no changes"), None);
    }

    #[test]
    fn test_extract_test_results() {
        let output = "10 passed, 2 failed, 1 skipped";
        assert_eq!(extract_test_results(output), Some((10, 2, 1)));
    }
}
