use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::Command;

use super::heuristics::{AgentStatus, StateInferenceEngine};
use super::TmuxSession;

/// Client for interacting with tmux via CLI
pub struct TmuxClient {
    /// Path to tmux binary
    tmux_path: String,
}

impl TmuxClient {
    pub fn new() -> Self {
        Self {
            tmux_path: "tmux".to_string(),
        }
    }

    /// Check if tmux server is running
    pub async fn is_server_running(&self) -> bool {
        Command::new(&self.tmux_path)
            .arg("list-sessions")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// List all tmux sessions
    pub async fn list_sessions(&self) -> Result<Vec<TmuxSession>> {
        // Format: session_id|session_name|session_created|session_attached
        let output = Command::new(&self.tmux_path)
            .args([
                "list-sessions",
                "-F",
                "#{session_id}|#{session_name}|#{session_created}|#{session_attached}",
            ])
            .output()
            .await
            .context("Failed to execute tmux list-sessions")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("no server running") || stderr.contains("no sessions") {
                return Ok(Vec::new());
            }
            anyhow::bail!("tmux list-sessions failed: {}", stderr);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut sessions = Vec::new();

        for line in stdout.lines() {
            if let Some(session) = self.parse_session_line(line).await {
                sessions.push(session);
            }
        }

        Ok(sessions)
    }

    async fn parse_session_line(&self, line: &str) -> Option<TmuxSession> {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() < 4 {
            return None;
        }

        let id = parts[0].to_string();
        let name = parts[1].to_string();
        let created_at = parts[2].parse().unwrap_or(0);
        let attached_clients = parts[3].parse().unwrap_or(0);

        // Get pane content for status detection
        let status = self.get_session_status(&id).await.unwrap_or(AgentStatus::Unknown);

        Some(TmuxSession {
            id,
            name,
            created_at,
            attached_clients,
            status,
        })
    }

    /// Get the status of a session by analyzing pane content
    async fn get_session_status(&self, session_id: &str) -> Result<AgentStatus> {
        let output = Command::new(&self.tmux_path)
            .args(["capture-pane", "-p", "-t", session_id])
            .output()
            .await
            .context("Failed to capture pane")?;

        if !output.status.success() {
            return Ok(AgentStatus::Unknown);
        }

        let content = String::from_utf8_lossy(&output.stdout);
        Ok(StateInferenceEngine::analyze(&content))
    }

    /// Create a new session with isolated history
    pub async fn create_session(&self, name: &str) -> Result<TmuxSession> {
        let history_dir = dirs::home_dir()
            .unwrap_or_default()
            .join(".agent-deck")
            .join("history");

        // Ensure history directory exists
        tokio::fs::create_dir_all(&history_dir).await?;

        let history_file = history_dir.join(format!("{}.hist", name));

        let output = Command::new(&self.tmux_path)
            .args(["new-session", "-d", "-s", name])
            .env("HISTFILE", &history_file)
            .output()
            .await
            .context("Failed to create tmux session")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to create session: {}", stderr);
        }

        // Get the session info
        let sessions = self.list_sessions().await?;
        sessions
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("Session created but not found"))
    }

    /// Kill a session
    pub async fn kill_session(&self, session_id: &str) -> Result<()> {
        let output = Command::new(&self.tmux_path)
            .args(["kill-session", "-t", session_id])
            .output()
            .await
            .context("Failed to kill tmux session")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to kill session: {}", stderr);
        }

        Ok(())
    }

    /// Get the command to attach to a session (for external execution)
    pub fn attach_command(&self, session_id: &str) -> Vec<String> {
        vec![
            self.tmux_path.clone(),
            "attach-session".to_string(),
            "-t".to_string(),
            session_id.to_string(),
        ]
    }
}

impl Default for TmuxClient {
    fn default() -> Self {
        Self::new()
    }
}
