use crate::error::{HtMcpError, Result};
use crate::mcp::types::*;
use crate::tunnel::TunnelManager;
use ht_core::{api::http, pty, pty::Winsize, session::Session};
use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use tracing::{error, info, warn};

// Enhanced command type that supports responses
#[derive(Debug)]
pub enum SessionCommand {
    Input(Vec<ht_core::command::InputSeq>),
    Snapshot(oneshot::Sender<String>),
    Resize(usize, usize),
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub id: String,
    pub internal_id: Uuid,
    pub created_at: std::time::SystemTime,
    pub web_server_url: Option<String>,
    pub tunnel_url: Option<String>,
    pub is_alive: bool,
    pub command: Vec<String>,
    pub command_tx: Arc<mpsc::Sender<SessionCommand>>,
}

pub struct SessionManager {
    sessions: HashMap<String, SessionInfo>,
    tunnel_manager: TunnelManager,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            tunnel_manager: TunnelManager::new(),
        }
    }

    pub async fn create_session(&mut self, args: CreateSessionArgs) -> Result<serde_json::Value> {
        let session_id = Uuid::new_v4().to_string();
        let command = args.command.unwrap_or_else(|| vec!["bash".to_string()]);
        let enable_web_server = args.enable_web_server.unwrap_or(false);
        let enable_tunnel = args.enable_tunnel.unwrap_or(false);
        let internal_id = Uuid::new_v4();

        // Create channels for communication
        let (input_tx, input_rx) = mpsc::channel::<Vec<u8>>(1024);
        let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(1024);
        let (command_tx, mut command_rx) = mpsc::channel::<SessionCommand>(1024);
        let (clients_tx, mut clients_rx) = mpsc::channel(1);

        // Create a platform-agnostic terminal size
        // Using a helper function to maintain a clean interface
        let size = create_winsize(120, 40);
        let cols = size.ws_col as usize;
        let rows = size.ws_row as usize;

        // Start HTTP server if enabled - we need to clone clients_tx for the HTTP server
        let (web_server_url, tunnel_url, _clients_tx_for_session) = if enable_web_server {
            let port = self.find_available_port().await?;
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            let listener = TcpListener::bind(addr).map_err(|e| {
                HtMcpError::Internal(format!("Failed to bind to port {}: {}", port, e))
            })?;

            let url = format!("http://127.0.0.1:{}", port);

            // Clone clients_tx for the HTTP server
            let clients_tx_for_http = clients_tx.clone();

            // Start the HTTP server with HT's native implementation
            tokio::spawn(async move {
                if let Ok(server_future) = http::start(listener, clients_tx_for_http).await {
                    if let Err(e) = server_future.await {
                        error!("HTTP server error: {}", e);
                    }
                }
            });

            // Start tunnel if enabled
            let tunnel_url = if enable_tunnel {
                match self.tunnel_manager.create_simple_tunnel(port).await {
                    Ok(tunnel_info) => {
                        info!(
                            "Tunnel created for session {}: {}",
                            session_id, tunnel_info.url
                        );
                        Some(tunnel_info.url)
                    }
                    Err(e) => {
                        error!("Failed to create tunnel for session {}: {}", session_id, e);
                        None
                    }
                }
            } else {
                None
            };

            info!("Started HT native webserver on {}", url);
            (Some(url), tunnel_url, clients_tx)
        } else {
            (None, None, clients_tx)
        };

        // Start PTY process
        let command_str = command.join(" ");
        let _pty_handle = tokio::spawn(async move {
            match pty::spawn(command_str, size, input_rx, output_tx) {
                Ok(future) => {
                    if let Err(e) = future.await {
                        error!("PTY execution error: {}", e);
                    }
                }
                Err(e) => {
                    error!("PTY spawn error: {}", e);
                }
            }
        });

        // Start session event loop
        let session_id_clone = session_id.clone();
        tokio::spawn(async move {
            let mut session = Session::new(cols, rows);
            let mut serving = true;

            loop {
                tokio::select! {
                    // Handle output from PTY
                    output = output_rx.recv() => {
                        match output {
                            Some(data) => {
                                session.output(String::from_utf8_lossy(&data).to_string());
                            }
                            None => {
                                info!("PTY process exited for session {}", session_id_clone);
                                break;
                            }
                        }
                    }

                    // Handle commands from MCP
                    command = command_rx.recv() => {
                        match command {
                            Some(SessionCommand::Input(seqs)) => {
                                let data = ht_core::command::seqs_to_bytes(&seqs, session.cursor_key_app_mode());
                                if let Err(e) = input_tx.send(data).await {
                                    error!("Failed to send input to PTY: {}", e);
                                }
                            }
                            Some(SessionCommand::Snapshot(response_tx)) => {
                                // Get the current terminal text and send it back
                                let text = session.get_text();
                                let _ = response_tx.send(text);
                            }
                            Some(SessionCommand::Resize(cols, rows)) => {
                                session.resize(cols, rows);
                            }
                            None => {
                                info!("Command channel closed for session {}", session_id_clone);
                                break;
                            }
                        }
                    }

                    // Handle WebSocket clients (for webserver)
                    client = clients_rx.recv(), if serving => {
                        match client {
                            Some(client) => {
                                info!("New WebSocket client connected to session {}", session_id_clone);
                                client.accept(session.subscribe());
                            }
                            None => {
                                info!("Client channel closed for session {}", session_id_clone);
                                serving = false;
                            }
                        }
                    }
                }
            }
        });

        // Create the session info
        let session_info = SessionInfo {
            id: session_id.clone(),
            internal_id,
            created_at: std::time::SystemTime::now(),
            web_server_url: web_server_url.clone(),
            tunnel_url: tunnel_url.clone(),
            is_alive: true,
            command: command.clone(),
            command_tx: Arc::new(command_tx),
        };

        self.sessions.insert(session_id.clone(), session_info);

        let result = CreateSessionResult {
            session_id,
            message: "HT session created successfully".to_string(),
            web_server_enabled: enable_web_server,
            web_server_url,
            tunnel_enabled: enable_tunnel,
            tunnel_url,
        };

        info!("Created HT session with native webserver: {:?}", result);
        Ok(serde_json::to_value(result)?)
    }

    /// Find an available port for the webserver
    /// Uses port range 3618-3999 to avoid conflicts with common development servers
    /// (Next.js: 3000, React: 3001, etc.)
    async fn find_available_port(&self) -> Result<u16> {
        for port in 3618..3999 {
            if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", port)) {
                drop(listener);
                return Ok(port);
            }
        }
        Err(HtMcpError::Internal("No available ports found".to_string()))
    }

    pub async fn send_keys(&mut self, args: SendKeysArgs) -> Result<serde_json::Value> {
        let session = self
            .sessions
            .get(&args.session_id)
            .ok_or_else(|| HtMcpError::SessionNotFound(args.session_id.clone()))?;

        // Convert keys to InputSeq format using HT's native key parsing
        let input_seqs: Vec<ht_core::command::InputSeq> = args
            .keys
            .iter()
            .map(|key| ht_core::api::stdio::parse_key(key.clone()))
            .collect();

        // Send keys via the command channel
        session
            .command_tx
            .send(SessionCommand::Input(input_seqs))
            .await
            .map_err(|e| HtMcpError::Internal(format!("Failed to send keys: {}", e)))?;

        info!("Sent keys {:?} to session {}", args.keys, args.session_id);

        Ok(serde_json::json!({
            "success": true,
            "message": format!("Keys sent successfully to session {}", args.session_id),
            "sessionId": args.session_id,
            "keys": args.keys
        }))
    }

    pub async fn take_snapshot(&self, args: TakeSnapshotArgs) -> Result<serde_json::Value> {
        let session = self
            .sessions
            .get(&args.session_id)
            .ok_or_else(|| HtMcpError::SessionNotFound(args.session_id.clone()))?;

        info!("Taking snapshot for session {}", args.session_id);

        // Create a response channel for the snapshot
        let (response_tx, response_rx) = oneshot::channel();

        // Send snapshot command with response channel
        session
            .command_tx
            .send(SessionCommand::Snapshot(response_tx))
            .await
            .map_err(|e| HtMcpError::Internal(format!("Failed to send snapshot command: {}", e)))?;

        // Wait for the response with a timeout
        let snapshot = tokio::time::timeout(tokio::time::Duration::from_secs(5), response_rx)
            .await
            .map_err(|_| HtMcpError::Internal("Snapshot request timed out".to_string()))?
            .map_err(|e| HtMcpError::Internal(format!("Failed to receive snapshot: {}", e)))?;

        info!(
            "Received snapshot for session {}: {} chars",
            args.session_id,
            snapshot.len()
        );

        Ok(serde_json::json!({
            "sessionId": args.session_id,
            "snapshot": snapshot
        }))
    }

    pub async fn execute_command(&mut self, args: ExecuteCommandArgs) -> Result<serde_json::Value> {
        // Send command
        self.send_keys(SendKeysArgs {
            session_id: args.session_id.clone(),
            keys: vec![args.command.clone()],
        })
        .await?;

        // Send Enter
        self.send_keys(SendKeysArgs {
            session_id: args.session_id.clone(),
            keys: vec!["Enter".to_string()],
        })
        .await?;

        // Wait for command to execute
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // Take snapshot
        let snapshot_result = self
            .take_snapshot(TakeSnapshotArgs {
                session_id: args.session_id.clone(),
            })
            .await?;

        Ok(serde_json::json!({
            "command": args.command,
            "sessionId": args.session_id,
            "output": snapshot_result["snapshot"]
        }))
    }

    pub async fn list_sessions(&self) -> Result<serde_json::Value> {
        let sessions: Vec<serde_json::Value> = self
            .sessions
            .values()
            .map(|session| {
                serde_json::json!({
                    "id": session.id,
                    "isAlive": session.is_alive,
                    "createdAt": session.created_at.duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default().as_secs(),
                    "command": session.command,
                    "webServerUrl": session.web_server_url,
                    "tunnelUrl": session.tunnel_url
                })
            })
            .collect();

        Ok(serde_json::json!({
            "sessions": sessions,
            "count": sessions.len()
        }))
    }

    pub async fn close_session(&mut self, args: CloseSessionArgs) -> Result<serde_json::Value> {
        let session = self
            .sessions
            .remove(&args.session_id)
            .ok_or_else(|| HtMcpError::SessionNotFound(args.session_id.clone()))?;

        // Close the command channel to trigger session shutdown
        drop(session.command_tx);

        info!("Closed session {}", args.session_id);

        Ok(serde_json::json!({
            "success": true,
            "message": format!("Session {} closed successfully", args.session_id)
        }))
    }
}

/// Creates a Winsize struct with platform-appropriate fields
/// This function abstracts away platform differences in the Winsize struct
fn create_winsize(cols: u16, rows: u16) -> Winsize {
    #[cfg(unix)]
    {
        Winsize {
            ws_col: cols,
            ws_row: rows,
            ws_xpixel: 0,
            ws_ypixel: 0,
        }
    }

    #[cfg(windows)]
    {
        Winsize {
            ws_col: cols,
            ws_row: rows,
        }
    }
}
