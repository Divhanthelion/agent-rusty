use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Status of an AI agent session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentStatus {
    /// Agent is actively processing (spinning, thinking)
    Busy,
    /// Agent is idle, waiting at prompt
    Idle,
    /// Agent is waiting for user input (confirmation, question)
    WaitingForInput,
    /// Agent encountered an error
    Error,
    /// Status cannot be determined
    #[default]
    Unknown,
}

/// Compiled regex patterns for status detection
static RE_WAITING_INPUT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?mi)(^\s*>\s*$|Type a message|Press Enter|waiting for input|\? $|\[y/n\]|\(y/N\)|\(Y/n\))").unwrap()
});

static RE_BUSY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?mi)(Thinking\.{3}|Processing|Loading|Working|⠋|⠙|⠹|⠸|⠼|⠴|⠦|⠧|⠇|⠏|\.\.\.$)").unwrap()
});

static RE_ERROR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?mi)(^Error:|^error:|Exception|FAILED|panic|fatal|crash)").unwrap()
});

static RE_IDLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)(^\$\s*$|^❯\s*$|^>\s*$|claude>)").unwrap()
});

/// Engine for inferring agent status from pane content
pub struct StateInferenceEngine;

impl StateInferenceEngine {
    /// Analyze pane content and determine agent status
    pub fn analyze(content: &str) -> AgentStatus {
        // Check last ~20 lines for most recent status
        let lines: Vec<&str> = content.lines().rev().take(20).collect();
        let recent_content = lines.into_iter().rev().collect::<Vec<_>>().join("\n");

        // Priority order: Error > WaitingForInput > Busy > Idle > Unknown
        if RE_ERROR.is_match(&recent_content) {
            return AgentStatus::Error;
        }

        if RE_WAITING_INPUT.is_match(&recent_content) {
            return AgentStatus::WaitingForInput;
        }

        if RE_BUSY.is_match(&recent_content) {
            return AgentStatus::Busy;
        }

        if RE_IDLE.is_match(&recent_content) {
            return AgentStatus::Idle;
        }

        AgentStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_waiting_for_input() {
        let content = "Some output\n\n> ";
        assert_eq!(StateInferenceEngine::analyze(content), AgentStatus::WaitingForInput);

        let content = "Do you want to continue? [y/n]";
        assert_eq!(StateInferenceEngine::analyze(content), AgentStatus::WaitingForInput);
    }

    #[test]
    fn test_detect_busy() {
        let content = "Working on the task...\nThinking...";
        assert_eq!(StateInferenceEngine::analyze(content), AgentStatus::Busy);
    }

    #[test]
    fn test_detect_error() {
        let content = "Something went wrong\nError: connection refused";
        assert_eq!(StateInferenceEngine::analyze(content), AgentStatus::Error);
    }

    #[test]
    fn test_detect_idle() {
        let content = "Previous output\n$ ";
        assert_eq!(StateInferenceEngine::analyze(content), AgentStatus::Idle);
    }
}
