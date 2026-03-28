//! JSON stream parser for backend output
//!
//! Backend type detection and progress parsing are reserved for future output processing.

#![allow(dead_code)] // Reserved API: backend type detection for enhanced output processing

use tokio::io::{AsyncBufRead, AsyncBufReadExt};
use tracing::trace;

/// Maximum message size in bytes (1MB)
const MAX_MESSAGE_SIZE: usize = 1_048_576;

/// JSON stream parser
pub struct JsonStreamParser<R> {
    reader: R,
    line_buffer: String,
}

impl<R: AsyncBufRead + Unpin> JsonStreamParser<R> {
    /// Create a new JSON stream parser
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            line_buffer: String::with_capacity(4096),
        }
    }

    /// Get the next JSON event from the stream
    pub async fn next_event(&mut self) -> Option<Result<serde_json::Value, ParseError>> {
        loop {
            self.line_buffer.clear();

            match self.reader.read_line(&mut self.line_buffer).await {
                Ok(0) => return None, // EOF
                Ok(n) => {
                    if n > MAX_MESSAGE_SIZE {
                        return Some(Err(ParseError::MessageTooLarge(n, MAX_MESSAGE_SIZE)));
                    }

                    let line = self.line_buffer.trim();

                    // Skip empty lines and non-JSON lines
                    if line.is_empty() {
                        continue;
                    }

                    // Fast pre-check: must start with { or [
                    if !line.starts_with('{') && !line.starts_with('[') {
                        trace!(line = %line, "Skipping non-JSON line");
                        continue;
                    }

                    match serde_json::from_str(line) {
                        Ok(value) => return Some(Ok(value)),
                        Err(e) => {
                            trace!(error = %e, line = %line, "JSON parse error");
                            // Continue to next line on parse error
                            continue;
                        }
                    }
                }
                Err(e) => {
                    return Some(Err(ParseError::IoError(e.to_string())));
                }
            }
        }
    }
}

/// Parse errors
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Message too large: {0} bytes (max: {1})")]
    MessageTooLarge(usize, usize),

    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("IO error: {0}")]
    IoError(String),
}

/// Backend type detection from JSON structure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    Codex,
    Claude,
    Gemini,
    Opencode,
    Unknown,
}

/// Detect backend type from JSON event structure
pub fn detect_backend_type(value: &serde_json::Value) -> BackendType {
    // Claude format: has "type" field
    if value.get("type").is_some() {
        return BackendType::Claude;
    }

    // Codex format: has "event" field
    if value.get("event").is_some() {
        return BackendType::Codex;
    }

    // Gemini format: has "candidates" field
    if value.get("candidates").is_some() {
        return BackendType::Gemini;
    }

    // Opencode format: has "message" object
    if value.get("message").is_some_and(|m| m.is_object()) {
        return BackendType::Opencode;
    }

    BackendType::Unknown
}

/// Check if event indicates progress
pub fn is_progress_event(value: &serde_json::Value, backend_type: BackendType) -> bool {
    match backend_type {
        BackendType::Claude => value
            .get("type")
            .and_then(|t| t.as_str())
            .is_some_and(|t| t == "assistant" || t == "content_block_delta" || t == "tool_use"),
        BackendType::Codex => value
            .get("event")
            .and_then(|e| e.as_str())
            .is_some_and(|e| e == "message" || e == "tool_call"),
        BackendType::Gemini => value.get("candidates").is_some(),
        BackendType::Opencode => value
            .get("type")
            .and_then(|t| t.as_str())
            .is_some_and(|t| t == "message" || t == "tool_use"),
        BackendType::Unknown => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;

    #[tokio::test]
    async fn test_parse_json_lines() {
        let input = r#"{"type": "assistant", "content": "Hello"}
not json
{"type": "done"}
"#;
        let reader = BufReader::new(input.as_bytes());
        let mut parser = JsonStreamParser::new(reader);

        let event1 = parser.next_event().await.unwrap().unwrap();
        assert_eq!(event1["type"], "assistant");

        let event2 = parser.next_event().await.unwrap().unwrap();
        assert_eq!(event2["type"], "done");

        assert!(parser.next_event().await.is_none());
    }

    #[test]
    fn test_detect_backend_type() {
        let claude = serde_json::json!({"type": "assistant"});
        assert_eq!(detect_backend_type(&claude), BackendType::Claude);

        let codex = serde_json::json!({"event": "message"});
        assert_eq!(detect_backend_type(&codex), BackendType::Codex);

        let gemini = serde_json::json!({"candidates": []});
        assert_eq!(detect_backend_type(&gemini), BackendType::Gemini);

        let opencode = serde_json::json!({"message": {}});
        assert_eq!(detect_backend_type(&opencode), BackendType::Opencode);
    }

    #[test]
    fn test_is_progress_event() {
        let claude = serde_json::json!({"type": "assistant"});
        assert!(is_progress_event(&claude, BackendType::Claude));

        let codex = serde_json::json!({"event": "message"});
        assert!(is_progress_event(&codex, BackendType::Codex));
    }
}
