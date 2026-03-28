//! Utility functions
//!
//! These utilities are reserved for progress display and path handling.

#![allow(dead_code)] // Reserved API: progress formatting and path utilities for enhanced UX

use anyhow::Result;
use serde_json::{Value, json};
use std::env;

use crate::executor::TaskResult;

/// Generate final output JSON for a single task
pub fn generate_final_output(result: &TaskResult) -> Result<String> {
    let output = json!({
        "success": result.success,
        "exitCode": result.exit_code,
        "duration": result.duration.as_millis(),
        "sessionId": result.session_id,
        "filesChanged": result.files_changed,
        "coverage": result.coverage,
        "events": result.events,
    });

    Ok(serde_json::to_string_pretty(&output)?)
}

/// Generate final output JSON for parallel execution
pub fn generate_parallel_output(results: &[TaskResult]) -> Result<String> {
    let task_results: Vec<Value> = results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            json!({
                "taskIndex": i,
                "success": r.success,
                "exitCode": r.exit_code,
                "duration": r.duration.as_millis(),
                "sessionId": r.session_id,
            })
        })
        .collect();

    let all_success = results.iter().all(|r| r.success);
    let total_duration: u128 = results.iter().map(|r| r.duration.as_millis()).sum();

    let output = json!({
        "success": all_success,
        "totalTasks": results.len(),
        "successfulTasks": results.iter().filter(|r| r.success).count(),
        "failedTasks": results.iter().filter(|r| !r.success).count(),
        "totalDuration": total_duration,
        "tasks": task_results,
    });

    Ok(serde_json::to_string_pretty(&output)?)
}

/// Format progress message for display
pub fn format_progress_message(event: &Value, quiet: bool) -> Option<String> {
    if quiet {
        return None;
    }

    let use_ascii = env::var("CODEAGENT_ASCII_MODE").is_ok();

    let event_type = event.get("type").and_then(|t| t.as_str()).unwrap_or("");

    let symbol = if use_ascii {
        match event_type {
            "assistant" => "[>]",
            "tool_use" => "[*]",
            "error" => "[!]",
            "done" => "[+]",
            _ => "[-]",
        }
    } else {
        match event_type {
            "assistant" => "💬",
            "tool_use" => "🔧",
            "error" => "❌",
            "done" => "✅",
            _ => "📝",
        }
    };

    // Extract a short description
    let desc = event
        .get("content")
        .and_then(|c| c.as_str())
        .or_else(|| event.get("tool").and_then(|t| t.as_str()))
        .unwrap_or("...");

    // Truncate long descriptions
    let desc = if desc.len() > 60 {
        format!("{}...", &desc[..57])
    } else {
        desc.to_string()
    };

    Some(format!("{} {}", symbol, desc))
}

/// Expand ~ to home directory
pub fn expand_home(path: &str) -> String {
    if path.starts_with("~/")
        && let Some(home) = dirs::home_dir()
    {
        return format!("{}{}", home.display(), &path[1..]);
    }
    path.to_string()
}

/// Check if a string is a valid session ID
pub fn is_valid_session_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_generate_final_output() {
        let result = TaskResult {
            success: true,
            exit_code: 0,
            duration: Duration::from_millis(1234),
            session_id: Some("abc123".to_string()),
            ..Default::default()
        };

        let output = generate_final_output(&result).unwrap();
        let parsed: Value = serde_json::from_str(&output).unwrap();

        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["exitCode"], 0);
        assert_eq!(parsed["duration"], 1234);
        assert_eq!(parsed["sessionId"], "abc123");
    }

    #[test]
    fn test_expand_home() {
        let expanded = expand_home("~/test/path");
        assert!(!expanded.starts_with("~"));

        let unchanged = expand_home("/absolute/path");
        assert_eq!(unchanged, "/absolute/path");
    }

    #[test]
    fn test_is_valid_session_id() {
        assert!(is_valid_session_id("abc123"));
        assert!(is_valid_session_id("session-id_123"));
        assert!(!is_valid_session_id(""));
        assert!(!is_valid_session_id("invalid space"));
        assert!(!is_valid_session_id("invalid@char"));
    }

    #[test]
    fn test_format_progress_message() {
        let event = json!({"type": "assistant", "content": "Hello, I'm working on your task"});
        let msg = format_progress_message(&event, false);
        assert!(msg.is_some());

        let quiet_msg = format_progress_message(&event, true);
        assert!(quiet_msg.is_none());
    }
}
