# AgentRusty

A Rust TUI application for managing multiple AI coding agents via tmux. This is a rewrite of agent-deck from Go/Bubble Tea to Rust/ratatui.

## Project Overview

AgentRusty provides a terminal-based dashboard ("mission control") for orchestrating concurrent AI coding sessions. It wraps tmux session management with a TUI that visualizes agent states, manages MCP (Model Context Protocol) tool configurations, and provides status heuristics to detect whether agents are busy, idle, or waiting for input.

## Tech Stack

- **Runtime**: Tokio async runtime
- **TUI Framework**: ratatui with crossterm backend
- **Error Handling**: anyhow (application), thiserror (library errors)
- **Serialization**: serde, serde_json
- **MCP Integration**: mcp_rust_sdk or modelcontextprotocol/rust-sdk
- **File Traversal**: walkdir, ignore (for .gitignore support)
- **Regex**: regex crate with lazy_static/OnceLock for compiled patterns
- **Clipboard**: arboard

## Architecture

### Concurrency Model: Async Actor Pattern

The application uses an async actor pattern instead of Elm Architecture (TEA):

- **Input Actor**: Captures keyboard/mouse events via crossterm
- **Tmux Poller**: Periodically fetches session state via tmux CLI
- **State Heuristic Engine**: Analyzes pane content to determine agent status (CPU-bound, uses spawn_blocking)
- **MCP Manager**: Supervises MCP server child processes
- **Main Event Loop**: Aggregates channels via `tokio::select!`, updates state, renders UI

### State Management

Single-owner pattern for App state in main loop. Worker tasks send data snapshots via mpsc channels (double-buffering approach) - no Arc<Mutex<T>> for UI state.

## Key Features

1. **Session Management**: Create, list, attach/detach tmux sessions with isolated history files per agent
2. **Status Heuristics**: Regex-based detection of agent states (Busy/Idle/WaitingForInput) via `tmux capture-pane`
3. **MCP Configuration**: Dynamic tool injection per project without editing config files
4. **Skeleton Map**: Project structure visualization using walkdir, copyable to clipboard
5. **Guardian Middleware** (planned): Enforce quality gates and no-touch zones

## Project Structure (Planned)

```
src/
├── main.rs              # Entry point, runtime init, terminal setup
├── app.rs               # App state and main event loop
├── actions.rs           # Action enum for event handling
├── components/          # UI components implementing Component trait
│   ├── mod.rs
│   ├── session_list.rs  # Session list with status icons
│   ├── detail_pane.rs   # Live preview or skeleton map
│   ├── mcp_status.rs    # MCP tool sidebar
│   └── log_console.rs   # Debug/log viewer
├── tmux/                # Tmux integration
│   ├── mod.rs
│   ├── client.rs        # TmuxClient for CLI commands
│   └── heuristics.rs    # StateInferenceEngine
├── mcp/                 # MCP integration
│   ├── mod.rs
│   ├── registry.rs      # Tool registry (registry.toml)
│   └── proxy.rs         # Optional MCP proxy server
├── skeleton/            # Skeleton map feature
│   └── mod.rs
└── config/              # Configuration handling
    └── mod.rs
```

## Commands

```bash
# Build
cargo build

# Run
cargo run

# Run with release optimizations
cargo run --release

# Run tests
cargo test

# Check for errors without building
cargo check

# Format code
cargo fmt

# Lint
cargo clippy
```

## Key Bindings (Planned)

- `Enter` - Attach to selected session
- `n` - New session
- `d` - Delete session
- `M` - Toggle MCP mode
- `Space` - Toggle selection in lists
- `j/k` - Navigate up/down
- `y` - Copy skeleton map to clipboard
- `q` - Quit

## Important Implementation Notes

- Terminal cleanup must happen even on panic (use `ratatui::restore()` in drop or defer)
- Heuristic regex patterns should be compiled once (lazy_static/OnceLock)
- Status polling should be debounced (~500ms) to prevent high CPU usage
- Use `-F` format flag for tmux commands to ensure consistent parsing
- Session attachment requires suspending TUI, inheriting stdin/stdout, then resuming

## Configuration Files

- `~/.agent-deck/history/<session_id>.hist` - Isolated shell history per session
- `~/.agent-deck/registry.toml` - MCP tool registry
- `/tmp/agent-deck/configs/<session_id>.json` - Transient Claude config per session

## Testing Strategy

- Unit tests for heuristic regex patterns and tmux output parsing
- Mock `TmuxBackend` trait for integration tests without real tmux
- Test with ANSI escape codes in captured pane content
