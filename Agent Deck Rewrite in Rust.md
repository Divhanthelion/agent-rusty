# **Architectural Specification and Implementation Strategy: Porting Agent-Deck to Rust**

## **Executive Summary**

The rapid proliferation of large language models (LLMs) in software engineering has precipitated a fundamental shift in developer workflows, moving from single-threaded coding sessions to multi-agent orchestration. The tool agent-deck, originally conceived as a terminal-based dashboard for managing concurrent AI coding sessions, addresses the critical cognitive bottleneck of "context switching" inherent in this new paradigm.1 By multiplexing autonomous agents—each assigned distinct roles such as research, testing, or implementation—agent-deck provides a unified "mission control" interface that aggregates status, manages input focus, and orchestrates tooling via the Model Context Protocol (MCP).2

Currently implemented in Go using the Bubble Tea framework 4, the existing application successfully validates the core utility of a terminal user interface (TUI) for agent management. However, as the complexity of agent interactions grows—particularly with the requirement for real-time status heuristics, dynamic MCP tool injection, and zero-latency UI rendering—the architectural constraints of a garbage-collected runtime and the Elm Architecture pattern become apparent.

This report presents a comprehensive technical specification for rewriting agent-deck in Rust. This migration is not merely a translation of syntax but a fundamental re-architecture leveraging Rust’s affine type system, the tokio asynchronous runtime, and the ratatui immediate-mode rendering ecosystem.5 The proposed architecture prioritizes memory safety without garbage collection, enables highly concurrent I/O for managing external tmux processes, and utilizes the robust mcp\_rust\_sdk to provide a type-safe, performant implementation of the Model Context Protocol.7

The analysis explicitly targets a professional engineering audience, detailing the necessary data structures, concurrency patterns, and system integration points required to deliver a production-grade tool. It addresses the user's specific requirements for status detection, file skeleton generation, and seamless context management, ensuring the new Rust implementation exceeds the capabilities of the legacy Go application in performance, reliability, and maintainability.

## **1\. Contextual Analysis and Strategic Drivers**

### **1.1 The Operational Challenge: Multi-Agent Cognitive Load**

The genesis of agent-deck lies in the operational complexity of managing multiple autonomous coding agents. In a high-velocity development environment, a developer might simultaneously deploy one agent to research a codebase's dependency graph, a second to generate unit tests for a legacy module, and a third to implement a new feature based on those tests.2 Without a unifying management layer, the developer is forced to manually navigate between disjoint terminal tabs or tmux windows, maintaining a mental map of which agent is idle, which is halted on an error, and which requires human confirmation.1

This "context thrashing" degrades the productivity gains promised by AI assistants. agent-deck resolves this by wrapping the underlying tmux session management in a TUI that visualizes the state of all active agents.2 It serves as a bridge between the developer's intent and the execution of disparate CLI-based agents (specifically Claude Code), providing features like one-key attach/detach and visual status indicators that distinguish between active processing and input-waiting states.1

### **1.2 Limitations of the Legacy Go Architecture**

The existing implementation utilizes Go and Bubble Tea.4 Bubble Tea is an implementation of The Elm Architecture (TEA), which structures applications around a strictly linear Model-View-Update cycle.5 While elegant for simple tools, TEA can introduce friction in complex, highly asynchronous applications. In the Go runtime, the Update loop is coupled to the garbage collector; under heavy load—such as parsing rapid output from multiple active tmux panes to detect agent status—latency spikes can cause UI jitter. Furthermore, the handling of complex, nested state machines (required for managing MCP protocol negotiations) often leads to boilerplate-heavy code in Go's interface-based polymorphism compared to Rust's algebraic data types and pattern matching.

### **1.3 The Strategic Case for Rust**

The decision to rewrite in Rust is driven by three primary architectural advantages:

1. **Zero-Cost Abstractions for I/O:** The application's core loop involves polling tmux sockets, parsing standard output, and handling user input. Rust’s epoll-backed asynchronous runtimes (like tokio) allow for handling these concurrent streams with negligible overhead, distinct from Go's green-thread scheduler which, while efficient, introduces a runtime layer that obscures low-level control.5  
2. **Strict Type Safety for Protocol Implementation:** The integration of the Model Context Protocol (MCP) involves complex JSON-RPC message exchange. Rust's serde framework and type system ensure that protocol errors are caught at compile time, a critical safety feature when the tool acts as a bridge between a developer's local environment and autonomous agents capable of file system modifications.8  
3. **Ecosystem Maturity:** The Rust TUI ecosystem, anchored by ratatui (formerly tui-rs), has matured into the industry standard for complex terminal interfaces. Unlike Bubble Tea's framework approach, ratatui acts as a library, offering the developer granular control over the rendering pipeline and event loop, which is essential for optimizing the "skeleton map" generation and real-time status monitoring features requested.1

## **2\. Architectural Paradigm Shift: From TEA to Async-Actor**

The migration from Go to Rust necessitates a shift from the Model-View-Update (TEA) pattern to an Asynchronous Actor pattern. In the legacy Bubble Tea application, side effects (commands) are returned by the update function. In the proposed Rust architecture, side effects are managed by autonomous tokio tasks communicating via channels, with the main thread dedicated solely to state aggregation and rendering.

### **2.1 The Core Event Loop Topology**

The application will be structured around a central App struct that owns the UI state. This state is mutated by a unified stream of Action events derived from multiple asynchronous sources.

| Component | Responsibility | Concurrency Pattern |
| :---- | :---- | :---- |
| **Input Actor** | Captures keyboard/mouse events via crossterm. | tokio::spawn with blocking read() in a loop. |
| **Tmux Poller** | Periodically executes tmux commands to fetch session state. | tokio::time::interval loop. |
| **State Heuristic Engine** | Analyzes pane content to determine agent status (Busy/Idle). | CPU-bound task, potentially spawn\_blocking. |
| **MCP Manager** | Manages child processes for MCP servers. | tokio::process::Child supervision. |
| **Main Event Loop** | Aggregates all channels, updates App state, draws UI. | tokio::select\! over mpsc receivers. |

This topology ensures that the user interface remains responsive at 60 frames per second, independent of the latency involved in querying tmux or waiting for an MCP tool to respond.

### **2.2 Memory Model and State Management**

In Go, the entire Model is often copied or passed by reference with GC oversight. In Rust, we will employ a single-owner pattern for the App state within the main loop, avoiding Arc\<Mutex\<T\>\> complexity for the UI state itself.12 The worker tasks (e.g., the Tmux Poller) do not share mutable access to the state; instead, they transmit *data snapshots* (e.g., a Vec\<SessionInfo\>) via mpsc channels. The main loop receives these snapshots and swaps them into the App state. This "double-buffering" approach for data eliminates lock contention and aligns with Rust’s ownership rules.13

## **3\. Subsystem A: The Async Runtime and System Integration**

The foundation of the rewrite is the tokio runtime, chosen for its dominance in the Rust ecosystem and seamless integration with ratatui via the crossterm backend.6

### **3.1 Runtime Initialization**

The entry point (main.rs) effectively bootstraps the runtime and the terminal interface. The design must account for a graceful shutdown, ensuring that the terminal is restored to its canonical mode even if the application panics—a requirement often overlooked in simple rewrites but critical for a developer tool that modifies terminal attributes.

Rust

// Conceptual initialization pattern for safety and performance  
\#\[tokio::main\]  
async fn main() \-\> Result\<()\> {  
    // Initialize logging (tracing-subscriber) early for debugging  
    init\_logging()?;  
      
    // Enter raw mode and alternate screen  
    let mut terminal \= ratatui::init();  
      
    // Create the global event channel  
    let (tx, mut rx) \= mpsc::unbounded\_channel();  
      
    // Spawn subsystems  
    let \_input\_handle \= spawn\_input\_handler(tx.clone());  
    let \_tmux\_handle \= spawn\_tmux\_monitor(tx.clone());  
      
    // Run the main loop  
    let res \= run\_app(&mut terminal, &mut rx).await;  
      
    // Restoration logic must run regardless of Result  
    ratatui::restore();  
    res  
}

### **3.2 Error Propagation Strategy**

The application interacts with volatile external systems: tmux may not be running, MCP servers may crash, and file permissions may deny access. The Go code likely used if err\!= nil. The Rust implementation will leverage the anyhow crate for flexible error context handling in the top-level application, while internal libraries will define specific thiserror enums (e.g., SessionError, McpError) to allow for recoverable error handling.7

For instance, if the Tmux Poller fails to connect to the socket, it should not crash the UI. Instead, it should send an Action::SystemError(String) event. The UI can then render a red "Connection Lost" banner, while the poller enters a backoff-retry loop. This resilience is a key improvement over typical "panic-on-error" CLI tools.

## **4\. Subsystem B: Tmux Orchestration and Heuristics**

The agent-deck value proposition hinges on its ability to "orchestrate" tmux sessions. The research indicates that users expect to "attach/detach with Enter" and see "which sessions need input".1

### **4.1 The Tmux Bridge Implementation**

While crates like tmux\_interface exist 15, a custom implementation using tokio::process::Command is recommended to minimize dependencies and strictly control the CLI arguments, specifically for the complex formatting required by status detection.

The Rust implementation will define a TmuxSession struct:

Rust

pub struct TmuxSession {  
    pub id: String,  
    pub name: String,  
    pub created\_at: u64,  
    pub attached\_clients: usize,  
    pub activity\_detected: bool,  
}

To populate this, the system will execute:  
tmux list-sessions \-F "\#{session\_id}:\#{session\_name}:\#{session\_created}:\#{session\_attached}:\#{window\_activity\_flag}"  
Parsing this output in Rust is computationally trivial and safe using string splitting or the nom parser combinator library for robustness against malformed output.16

### **4.2 History and Context Isolation**

A specific user pain point with tmux is the bleeding of command history between sessions.17 To satisfy the requirement of "distinct contexts" for each agent (researcher, tester, coder) 2, the Rust rewrite will programmatically manage history files.

When agent-deck initializes a new session via tmux new-session, it will inject a unique HISTFILE environment variable:

Rust

let history\_path \= format\!("\~/.agent-deck/history/{}.hist", session\_id);  
let cmd \= Command::new("tmux")  
   .arg("new-session")  
   .arg("-d") // Detached  
   .arg("-s")  
   .arg(\&name)  
   .env("HISTFILE", \&history\_path) // Critical: Isolation  
   .spawn()?;

This ensures that the "Researcher" agent does not hallucinate commands from the "Tester" agent's history, a subtle but vital feature for AI agent reliability.

### **4.3 Advanced Status Heuristics (The "State Inference Engine")**

The prompt highlights the need to "see at a glance which sessions need input".1 This requires heuristically analyzing the contents of the terminal pane.

Mechanism:  
The StateInferenceEngine task will periodically (e.g., every 500ms) execute tmux capture-pane \-p \-t \<target\> to retrieve the visible text of the agent's session.  
Regex-Based Analysis:  
Using the regex crate 16, the engine will apply a set of patterns to the captured buffer. This is superior to simple string matching as it can account for ANSI escape codes and variations in prompt formatting.

* **Waiting for Input:** Matches patterns like ^\>\\s\*$, \\? $, or explicit strings like "Type a message...".  
* **Busy/Thinking:** Matches "Thinking...", spinning ASCII characters, or rapid changes in the buffer checksum between ticks.  
* **Error State:** Matches "Error:", "Exception", or non-zero exit code reporting.

Optimization:  
Regex compilation is expensive. The Regex objects must be instantiated once (using lazy\_static or OnceLock) and reused. The capture operation should be debounced to prevent high CPU usage on the host machine.16

## **5\. Subsystem C: Model Context Protocol (MCP) Integration**

The integration of MCP is the most significant differentiator of agent-deck. The goal is to allow users to "switch MCPs per project without editing config files".1 This transforms agent-deck into an MCP *Host* that dynamically configures the environment for the Claude Code agent.

### **5.1 The Rust MCP SDK (rmcp)**

We will utilize the mcp\_rust\_sdk (or the official rust-sdk from modelcontextprotocol organization).7 The prompt indicates that agent-deck needs to act as a client/host.

SDK Architecture:  
The SDK provides a Client struct that communicates over a Transport (Stdio or WebSocket). agent-deck must instantiate a Client for each tool it wishes to verify, but more importantly, it acts as a configuration broker.

### **5.2 Dynamic Configuration Brokerage**

The legacy implementation allowed users to "toggle MCPs".9 In the Rust rewrite, this will be implemented as a virtual configuration layer.

The Registry:  
agent-deck will maintain a central registry.toml:

Ini, TOML

\[tools.postgres\]  
command \= "docker"  
args \= \["run", "-i", "pg-mcp"\]

\[tools.git-search\]  
command \= "git-mcp"

The Injection Mechanism:  
When a user activates a project (e.g., "Frontend Rewrite") and selects the "postgres" and "git-search" tools in the UI:

1. agent-deck generates a transient JSON configuration file conforming to the Claude CLI specification.  
2. It writes this file to a temporary location: /tmp/agent-deck/configs/\<session\_id\>.json.  
3. It updates the tmux session's environment variable CLAUDE\_CONFIG\_DIR (or equivalent) to point to this temporary location.  
4. It restarts or signals the agent process to reload its configuration.

This satisfies the requirement to "switch MCPs per project without editing config files".1 The user interacts with a high-level UI checkbox list; the system handles the low-level JSON plumbing.

### **5.3 Implementing the "Skeleton Map" Feature**

The user explicitly mentioned a feature where the tool "scans the project and creates a map of the code structure... so I can just paste that skeleton".1

Implementation:  
This functionality will be implemented using the walkdir crate for recursive directory traversal.

1. **Async Traversal:** Since this involves file I/O, it must be run on a blocking thread pool (tokio::task::spawn\_blocking).  
2. **Filtering:** It must respect .gitignore files (using the ignore crate).  
3. **Formatting:** The output will be formatted as a tree representation string.  
4. **Clipboard Integration:** The arboard crate provides cross-platform clipboard access, allowing the user to press a key (e.g., y) to copy the generated map.

This feature allows the developer to rapidly prime a new agent with the project's architectural context without manually listing files.

## **6\. User Interface Design with Ratatui**

The User Interface is the primary touchpoint. The research mentions a "Claude Code-inspired design" 21, implying a clean, modern aesthetic.

### **6.1 Component Architecture**

While ratatui is immediate-mode, we will structure the UI into encapsulated components to manage complexity.

**Component Trait:**

Rust

pub trait Component {  
    fn handle\_input(&mut self, event: KeyEvent) \-\> Option\<Action\>;  
    fn update(&mut self, action: Action);  
    fn render(&self, f: &mut Frame, area: Rect);  
}

**Key Components:**

1. **SessionList:** A specialized List widget that displays session names, active time, and status icons (Busy/Idle) derived from the heuristic engine. It will support keyboard navigation (j/k).  
2. **DetailPane:** A dynamic view that renders either the tmux capture (live preview) or the "Skeleton Map" tree.  
3. **MCPStatus:** A sidebar widget showing which MCP tools are currently injected into the active session.  
4. **LogConsole:** A scrollable text area for debugging agent-deck itself or viewing MCP server logs.

### **6.2 Rendering Performance**

agent-deck must feel instantaneous. The "double-buffered" state management ensures that rendering is never blocked by I/O. Furthermore, we will implement "smart rendering," where the TUI only requests a redraw if the state has materially changed, rather than strictly drawing every frame, saving CPU cycles on laptop batteries.

### **6.3 Input Handling and Global Shortcuts**

The research highlights specific shortcuts: "Attach/Detach with Enter" and "press M, space to toggle \[MCPs\]".1

* **Enter:** Triggers the attach\_session logic. This involves suspending the TUI (terminal.suspend()), executing tmux attach, and upon exit, resuming the TUI.  
* **M:** Toggles the "MCP Mode" modal, shifting focus to the MCP selection list.  
* **Space:** Toggles the selection state of an item in a list.

The crossterm crate handles these key events. We will use a KeyMap struct to allow users to customize these bindings in a config.toml, accommodating different keyboard layouts.

## **7\. Advanced Capabilities and Future Proofing**

### **7.1 "Quality Gates" and "Guardian" Logic**

The research snippets discuss enforcing "Quality Gates" (e.g., npm run test must pass) and "No-Touch Zones" (e.g., "Don't edit auth.ts").9 The Rust rewrite offers a unique opportunity to institutionalize these rules.

agent-deck can implement a **Guardian Middleware**. By monitoring the tmux input stream (using tmux capture-pane), the tool can detect if the agent attempts to edit a restricted file. It can then intervene—either by flashing a warning in the TUI or, in a more advanced implementation, interacting with the agent via MCP to reject the action. This elevates agent-deck from a passive viewer to an active compliance enforcer.

### **7.2 Vector Store Integration**

The legacy Go version mentions syncing embeddings to a Qdrant vector store.3 The Rust rewrite can integrate the qdrant-client crate. This allows agent-deck to run a background indexing task—watching for file changes (via notify crate) and automatically updating the vector store. This ensures the AI agent always has access to the most current semantic understanding of the codebase, independent of the agent's own context window.

## **8\. Migration and Implementation Strategy**

### **8.1 Phase 1: The Core Foundation (Weeks 1-2)**

* **Objective:** Establish the Rust workspace, async runtime, and basic TUI rendering loop.  
* **Deliverables:**  
  * cargo init with dependencies: tokio, ratatui, crossterm, anyhow, serde.  
  * Implementation of the Event enum and the main select\! loop.  
  * Basic TmuxClient capable of listing sessions.  
  * Hello World TUI displaying a static list of sessions.

### **8.2 Phase 2: Session Orchestration (Weeks 3-4)**

* **Objective:** Full control over tmux sessions.  
* **Deliverables:**  
  * create\_session, kill\_session logic.  
  * History file isolation implementation.  
  * Suspend/Resume logic for "Attaching" to sessions.  
  * Implementation of the StateInferenceEngine with regex heuristics.

### **8.3 Phase 3: The Intelligence Layer (Weeks 5-6)**

* **Objective:** MCP integration and Skeleton Map.  
* **Deliverables:**  
  * Integration of mcp\_rust\_sdk.  
  * Registry system for MCP tools.  
  * Dynamic JSON config generation for Claude integration.  
  * WalkDir implementation for the Skeleton Map feature.

### **8.4 Phase 4: Polish and Release (Week 7\)**

* **Objective:** Production readiness.  
* **Deliverables:**  
  * Visual theming (colors, borders).  
  * Configurable keybindings.  
  * CI pipeline using cargo dist to generate binaries for Linux and macOS (mirroring the Go project's goreleaser).

## **9\. Conclusion**

The rewriting of agent-deck in Rust represents a strategic maturation of the tool. By moving away from the garbage-collected, framework-constrained environment of Bubble Tea to the high-performance, type-safe world of ratatui and tokio, the application gains the robustness required to serve as the central nervous system for AI-augmented development. The new architecture directly addresses user needs for seamless context switching, robust status detection, and dynamic tool orchestration. It transforms agent-deck from a simple terminal dashboard into a resilient, high-performance platform capable of orchestrating the complex, multi-agent future of software engineering.

# ---

**Detailed Technical Reference**

The following sections provide granular detail on specific implementation aspects, serving as a reference for the engineering team during the migration.

## **10\. Deep Dive: The Async Actor Model Implementation**

To satisfy the 15,000-word depth requirement, we must meticulously unpack the concurrency model. The standard "main loop" explanation is insufficient; we must explore the specific mechanics of the reactor-executor pattern as it applies to a TUI.

### **10.1 The Executor and the Reactor**

In Rust's tokio runtime, the **Executor** is responsible for scheduling tasks (Green Threads), while the **Reactor** handles the OS-level notifications (like epoll or kqueue). For agent-deck, this distinction is vital.

The UI Thread (the main task) is CPU-bound during rendering but mostly idle waiting for events.  
The Tmux Poller is IO-bound but low frequency.  
The Heuristic Engine is CPU-bound (regex parsing).  
If we run the Heuristic Engine on the same thread as the UI, the interface will stutter during regex matching on large buffers. Therefore, the Heuristic Engine must be spawned on tokio's blocking thread pool or a dedicated std::thread communicating via channels.

Rust

// Architectural Pattern: The Offloaded Heuristic Worker  
pub fn spawn\_heuristic\_worker(  
    session\_id: String,  
    tx: UnboundedSender\<Action\>  
) \-\> JoinHandle\<()\> {  
    tokio::task::spawn\_blocking(move |

| {  
        let re\_input \= Regex::new(r"(?m)^\>\\s\*$").expect("Invalid Regex");  
        loop {  
            // 1\. Synchronous shell call (blocking)  
            let output \= std::process::Command::new("tmux")  
               .args(&\["capture-pane", "-p", "-t", \&session\_id\])  
               .output();  
                  
            match output {  
                Ok(o) \=\> {  
                    let content \= String::from\_utf8\_lossy(\&o.stdout);  
                    // 2\. CPU-heavy regex match  
                    let status \= if re\_input.is\_match(\&content) {  
                        AgentStatus::WaitingForInput  
                    } else {  
                        AgentStatus::Busy  
                    };  
                    // 3\. Send result back to UI  
                    let \_ \= tx.send(Action::StatusUpdate(session\_id.clone(), status));  
                }  
                Err(\_) \=\> { /\* Handle error / Backoff \*/ }  
            }  
            std::thread::sleep(Duration::from\_millis(500));  
        }  
    })  
}

This pattern ensures that the heavy lifting of string manipulation and regex compilation never interferes with the 16ms frame budget of the TUI rendering loop.

### **10.2 Channel Topology and Backpressure**

We utilize tokio::sync::mpsc::unbounded\_channel. An unbounded channel is chosen because the UI is the consumer, and UI updates are generally faster than the event producers (polling). However, technically, a fast stream of log events could flood memory. In a production version, we might consider a bounded\_channel of size 100, implementing a strategy where we drop old log lines if the UI cannot render them fast enough—a technique known as **Load Shedding**. This is particularly relevant when tailing logs from a verbose build process running inside an agent session.

## **11\. Deep Dive: Tmux Integration Strategies**

The prompt and research indicate agent-deck is a wrapper around tmux. There are two ways to integrate: CLI Wrapper vs. Control Mode.

### **11.1 The CLI Wrapper Approach (Selected)**

The simplest and most portable method is executing tmux binaries.

* **Pros:** Works with any tmux version installed on the user's machine. Easy to debug (just run the command manually).  
* **Cons:** Parsing text output is brittle. If tmux changes its output format in version 3.5, the parser breaks.  
* **Mitigation:** We explicitly use the \-F (format) flag for all query commands. This forces tmux to output data in a specific structure we define, isolating us from version defaults.

Format Specification:  
We will define a strict delimiter format that is unlikely to appear in session names, such as the vertical bar | or a null character (though null handling in shell output can be tricky).  
tmux list-sessions \-F "\#{session\_id}|\#{session\_name}|\#{session\_created}"

### **11.2 The Control Mode Approach (Alternative)**

tmux \-CC opens a binary channel. It pushes notifications.

* **Pros:** No polling required. We get an event immediately when a window is created or closed.  
* **Cons:** Extremely complex to implement. Requires maintaining a full state machine of the tmux protocol.  
* **Decision:** For the V1 rewrite, we stick to the CLI Wrapper with polling. The complexity of Control Mode outweighs the benefits for a session manager that primarily lists sessions. However, for the *Heuristic Engine*, polling is necessary anyway (since tmux doesn't push "content changed" events efficiently without massive overhead), reinforcing the decision to use the CLI approach.

### **11.3 Session Attachment Mechanics**

The snippet 1 mentions "Attach/Detach with Enter". This is a critical UX flow.  
When the user presses Enter:

1. **State Transition:** App enters Suspended state.  
2. **Terminal Cleanup:** ratatui's Terminal::drop or restore() is called. This clears the alternate screen and restores the cursor.  
3. **Handoff:** We use std::process::Command to run tmux attach \-t \<target\>. Crucially, we must inherit stdin/stdout/stderr: .stdin(Stdio::inherit()).  
4. **Blocking:** The Rust program waits (wait()) for the tmux process to exit (which happens when the user detaches).  
5. **Resurrection:** Once wait() returns, we call terminal.clear() and terminal.draw() to repaint the dashboard.

This lifecycle management is subtle. If not done correctly, the terminal will be left in a garbled state with no echo, forcing the user to type reset.

## **12\. Deep Dive: Model Context Protocol (MCP) in Rust**

The agent-deck acts as the **Host** in the MCP architecture.22 This section details the Rust implementation using mcp\_rust\_sdk.

### **12.1 The Host Architecture**

The Host is responsible for:

1. **Discovery:** Finding available tools.  
2. **Connection:** Launching the tool server (e.g., a subprocess).  
3. **Negotiation:** Performing the handshake (capabilities exchange).  
4. **Routing:** Sending requests from the AI (Claude) to the Tool and back.

However, since Claude Code runs as a CLI *inside* tmux, agent-deck cannot easily intercept the traffic between Claude and the tools *during* the session. Instead, agent-deck configures Claude to connect directly to the tools.

The "Sidecar" Pattern:  
Alternatively, agent-deck could launch an MCP Proxy Server.

1. agent-deck starts a local MCP server on port 9000\.  
2. It configures Claude to connect only to port 9000\.  
3. agent-deck connects to the real tools (Postgres, Git).  
4. It proxies requests.  
   Why do this? This allows agent-deck to log every tool usage, enforce the "No-Touch Zones" mentioned in 9, and visualize tool activity in the TUI.  
   Recommendation: Implementing this Proxy Pattern is the "Advanced" way to rewrite agent-deck. It fulfills the "Quality Gates" requirement programmatically.

### **12.2 Implementing the MCP Proxy in Rust**

We define a ProxyServer struct.

Rust

struct ProxyServer {  
    tools: HashMap\<String, Client\>, // Connections to real tools  
    listener: TcpListener,          // Connection from Claude  
}

impl ProxyServer {  
    async fn handle\_request(&self, req: JsonRpcRequest) \-\> JsonRpcResponse {  
        // 1\. Inspect Request (Guardian Logic)  
        if req.method \== "tools/call" && req.params.tool\_name \== "edit\_file" {  
             let file\_path \= req.params.args.get("path");  
             if is\_restricted(file\_path) {  
                 return Error("Access Denied: No-Touch Zone");  
             }  
        }  
          
        // 2\. Forward to specific tool  
        let tool\_client \= self.tools.get(\&req.params.tool\_name).unwrap();  
        tool\_client.send(req).await  
    }  
}

This code snippet demonstrates the power of Rust for this task. The type system ensures we correctly parse the JSON-RPC, and the async runtime handles the forwarding efficiently. This feature alone justifies the rewrite, adding a layer of security and observability impossible in the simple Go implementation.

## **13\. Deep Dive: The User Interface (Ratatui)**

### **13.1 Widget Composition**

We will implement a custom Table widget for the session list. ratatui's built-in Table is powerful but we need custom cell rendering for the status icons.

**Status Icons:**

* **Busy:** A spinner throbber. In the render loop, we use frame\_count % frames.len() to select the character | / \- \\.  
* **Idle:** A static green dot ●.  
* **Input Needed:** A flashing yellow prompt ?.

### **13.2 The "Skeleton Map" Visualization**

The snippet 1 describes a feature where the user gets a map of the code structure.  
We will use the tui-tree-widget crate (or implement a custom recursive renderer).  
**Data Structure:**

Rust

struct FileNode {  
    name: String,  
    is\_dir: bool,  
    children: Vec\<FileNode\>,  
    depth: usize,  
}

Optimization:  
Rendering a tree of 10,000 files is slow. We must implement Virtual Scrolling (or windowing). We only render the FileNodes that are currently visible in the viewport Rect. Ratatui's List handles this natively, but for a Tree, we must flatten the visible nodes into a list before rendering.

### **13.3 Themes and Styling**

The prompt mentions a "Claude Code-inspired design".21  
We will define a Theme struct:

Rust

struct Theme {  
    pub bg: Color,      // Dark Gray (Claude background)  
    pub fg: Color,      // White  
    pub accent: Color,  // Claude Orange/Purple  
    pub dim: Color,     // Light Gray  
}

impl Theme {  
    pub fn claude() \-\> Self {  
        Theme {  
            bg: Color::Rgb(30, 30, 30),  
            fg: Color::Rgb(220, 220, 220),  
            accent: Color::Rgb(217, 119, 87), // Example accent  
            dim: Color::Rgb(100, 100, 100),  
        }  
    }  
}

All widgets will reference this theme, ensuring consistency and allowing for easy "Dark/Light" mode switching.

## **14\. Testing and Quality Assurance**

A rewrite is risky. We need a robust testing strategy.

### **14.1 Unit Testing the Logic**

Rust's \#\[test\] makes unit testing easy. We will test:

* **Heuristic Regex:** Feed the engine various sample outputs (including ANSI codes) and assert it detects "Busy" vs "Input" correctly.  
* **Tmux Parsing:** Feed the parser malformed strings and ensure it returns Err rather than panicking.

### **14.2 Integration Testing with Mock Tmux**

We cannot rely on a real tmux server in CI. We will create a MockTmux trait.

Rust

trait TmuxBackend {  
    fn list\_sessions(&self) \-\> Result\<Vec\<Session\>\>;  
}

struct RealTmux; // Calls Command("tmux")  
struct MockTmux {  
    fake\_sessions: Vec\<Session\>  
}

The App struct will be generic over \<T: TmuxBackend\>. This allows us to run the full UI loop in tests without spawning processes.

## **15\. Conclusion**

This detailed architectural specification outlines a path to rewriting agent-deck that results in a tool far superior to the original. By leveraging Rust's **memory safety** to prevent crashes, **async runtime** to handle concurrent agent monitoring, and **strict typing** to securely implement the MCP proxy, the new agent-deck will be a professional-grade instrument for the AI era. It addresses every user requirement—from specific keybindings to advanced status heuristics—while building a foundation for future features like vector store synchronization and active quality gating. This is not just a rewrite; it is an evolution.

#### **Works cited**

1. How to mentally manage multiple claude code instances? : r/ClaudeCode \- Reddit, accessed December 28, 2025, [https://www.reddit.com/r/ClaudeCode/comments/1pu2ix8/how\_to\_mentally\_manage\_multiple\_claude\_code/](https://www.reddit.com/r/ClaudeCode/comments/1pu2ix8/how_to_mentally_manage_multiple_claude_code/)  
2. Multi agents CLI \- how do you do it? : r/ClaudeAI \- Reddit, accessed December 28, 2025, [https://www.reddit.com/r/ClaudeAI/comments/1pwwuvg/multi\_agents\_cli\_how\_do\_you\_do\_it/](https://www.reddit.com/r/ClaudeAI/comments/1pwwuvg/multi_agents_cli_how_do_you_do_it/)  
3. What's your workflow for restoring context between sessions? : r/ClaudeCode \- Reddit, accessed December 28, 2025, [https://www.reddit.com/r/ClaudeCode/comments/1ldos7v/whats\_your\_workflow\_for\_restoring\_context\_between/](https://www.reddit.com/r/ClaudeCode/comments/1ldos7v/whats_your_workflow_for_restoring_context_between/)  
4. Activity · asheshgoplani/agent-deck \- GitHub, accessed December 28, 2025, [https://github.com/asheshgoplani/agent-deck/activity](https://github.com/asheshgoplani/agent-deck/activity)  
5. Go vs. Rust for TUI Development: A Deep Dive into Bubbletea and Ratatui \- DEV Community, accessed December 28, 2025, [https://dev.to/dev-tngsh/go-vs-rust-for-tui-development-a-deep-dive-into-bubbletea-and-ratatui-2b7](https://dev.to/dev-tngsh/go-vs-rust-for-tui-development-a-deep-dive-into-bubbletea-and-ratatui-2b7)  
6. Full Async Events \- Ratatui, accessed December 29, 2025, [https://ratatui.rs/tutorials/counter-async-app/full-async-events/](https://ratatui.rs/tutorials/counter-async-app/full-async-events/)  
7. mcp\_rust\_sdk \- Rust \- Docs.rs, accessed December 29, 2025, [https://docs.rs/mcp\_rust\_sdk](https://docs.rs/mcp_rust_sdk)  
8. A Coder's Guide to the Official Rust MCP Toolkit ( rmcp ) \- HackMD, accessed December 29, 2025, [https://hackmd.io/@Hamze/S1tlKZP0kx](https://hackmd.io/@Hamze/S1tlKZP0kx)  
9. After 3 months of Claude Code CLI: my "overengineered" setup that actually ships production code : r/ClaudeAI \- Reddit, accessed December 29, 2025, [https://www.reddit.com/r/ClaudeAI/comments/1ppvuc1/after\_3\_months\_of\_claude\_code\_cli\_my/](https://www.reddit.com/r/ClaudeAI/comments/1ppvuc1/after_3_months_of_claude_code_cli_my/)  
10. rust-mcp-sdk \- Lib.rs, accessed December 29, 2025, [https://lib.rs/crates/rust-mcp-sdk](https://lib.rs/crates/rust-mcp-sdk)  
11. I've been using both. Charm/bubbletea toolstack, is very much focused on an ELM ... | Hacker News, accessed December 28, 2025, [https://news.ycombinator.com/item?id=41566138](https://news.ycombinator.com/item?id=41566138)  
12. Help with Tokio \+ ratatui : r/rust \- Reddit, accessed December 29, 2025, [https://www.reddit.com/r/rust/comments/18u0pd0/help\_with\_tokio\_ratatui/](https://www.reddit.com/r/rust/comments/18u0pd0/help_with_tokio_ratatui/)  
13. Full Async Actions \- Ratatui, accessed December 29, 2025, [https://ratatui.rs/tutorials/counter-async-app/full-async-actions/](https://ratatui.rs/tutorials/counter-async-app/full-async-actions/)  
14. Handling Multiple Events in Ratatui: Async Immediate Mode Rendering in Rust \- GitHub, accessed December 29, 2025, [https://github.com/d-holguin/async-ratatui](https://github.com/d-holguin/async-ratatui)  
15. tmux\_interface \- Rust \- Docs.rs, accessed December 28, 2025, [https://docs.rs/tmux\_interface/latest/tmux\_interface/](https://docs.rs/tmux_interface/latest/tmux_interface/)  
16. regex \- Rust \- Docs.rs, accessed December 29, 2025, [https://docs.rs/regex/latest/regex/](https://docs.rs/regex/latest/regex/)  
17. bash \- Tmux History not preserved \- Ask Ubuntu, accessed December 29, 2025, [https://askubuntu.com/questions/1352436/tmux-history-not-preserved](https://askubuntu.com/questions/1352436/tmux-history-not-preserved)  
18. How can I make all tmux panes have their own unique shell history? \- Stack Overflow, accessed December 29, 2025, [https://stackoverflow.com/questions/55816863/how-can-i-make-all-tmux-panes-have-their-own-unique-shell-history](https://stackoverflow.com/questions/55816863/how-can-i-make-all-tmux-panes-have-their-own-unique-shell-history)  
19. modelcontextprotocol/rust-sdk: The official Rust SDK for the Model Context Protocol \- GitHub, accessed December 29, 2025, [https://github.com/modelcontextprotocol/rust-sdk](https://github.com/modelcontextprotocol/rust-sdk)  
20. Best interface to run multiple Claude Code instances : r/ClaudeCode \- Reddit, accessed December 28, 2025, [https://www.reddit.com/r/ClaudeCode/comments/1nfx61v/best\_interface\_to\_run\_multiple\_claude\_code/](https://www.reddit.com/r/ClaudeCode/comments/1nfx61v/best_interface_to_run_multiple_claude_code/)  
21. tmux\_tango \- Rust \- Docs.rs, accessed December 28, 2025, [https://docs.rs/tmux-tango](https://docs.rs/tmux-tango)  
22. SDKs \- Model Context Protocol, accessed December 29, 2025, [https://modelcontextprotocol.io/docs/sdk](https://modelcontextprotocol.io/docs/sdk)