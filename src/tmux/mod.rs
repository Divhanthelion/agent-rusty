mod client;
mod heuristics;

pub use client::TmuxClient;
pub use heuristics::{AgentStatus, StateInferenceEngine};

use serde::{Deserialize, Serialize};

/// Represents a tmux session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmuxSession {
    /// Session ID (e.g., "$0")
    pub id: String,
    /// Session name
    pub name: String,
    /// Unix timestamp when session was created
    pub created_at: u64,
    /// Number of attached clients
    pub attached_clients: usize,
    /// Detected agent status
    pub status: AgentStatus,
}

impl TmuxSession {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            created_at: 0,
            attached_clients: 0,
            status: AgentStatus::Unknown,
        }
    }
}
