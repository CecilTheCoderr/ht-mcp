use crate::error::{HtMcpError, Result};
use crate::tunnel::config::TunnelConfig;
use regex::Regex;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};

/// Manages a Cloudflare tunnel instance
pub struct CloudflareTunnel {
    child: Child,
    pub url: String,
    pub local_port: u16,
}

impl CloudflareTunnel {
    /// Creates a new Cloudflare tunnel for the specified port
    /// Uses the simple TryCloudflare command: `cloudflared tunnel --url http://localhost:PORT`
    pub async fn new_simple(port: u16) -> Result<Self> {
        info!("Starting cloudflared tunnel on port {}", port);

        // Build the simple command following TryCloudflare documentation
        let mut cmd = Command::new("cloudflared");
        cmd.args(&["tunnel", "--url", &format!("http://localhost:{}", port)]);

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        // Spawn the process
        let mut child = cmd
            .spawn()
            .map_err(|e| HtMcpError::Internal(format!("Failed to spawn cloudflared: {}", e)))?;

        // Capture stderr to find the tunnel URL
        let stderr = child.stderr.take().ok_or_else(|| {
            HtMcpError::Internal("Failed to capture cloudflared stderr".to_string())
        })?;

        // Look for the tunnel URL in the output with 30 second timeout
        let timeout_duration = Duration::from_secs(30);
        let url = timeout(timeout_duration, Self::extract_tunnel_url(stderr))
            .await
            .map_err(|_| {
                HtMcpError::Internal("Timeout waiting for tunnel URL after 30s".to_string())
            })??;

        info!("Cloudflare tunnel established: {}", url);

        Ok(Self {
            child,
            url,
            local_port: port,
        })
    }

    /// Creates a new Cloudflare tunnel (legacy method for compatibility)
    pub async fn new(config: TunnelConfig) -> Result<Self> {
        Self::new_simple(config.port).await
    }

    /// Extracts the tunnel URL from cloudflared's stderr output
    async fn extract_tunnel_url(stderr: impl tokio::io::AsyncRead + Unpin) -> Result<String> {
        let mut reader = BufReader::new(stderr).lines();
        let url_regex = Regex::new(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com")
            .map_err(|e| HtMcpError::Internal(format!("Invalid regex: {}", e)))?;

        let mut attempts = 0;
        const MAX_ATTEMPTS: u32 = 100; // Prevent infinite loops

        while let Some(line) = reader.next_line().await.map_err(|e| {
            HtMcpError::Internal(format!("Failed to read cloudflared output: {}", e))
        })? {
            attempts += 1;
            if attempts > MAX_ATTEMPTS {
                return Err(HtMcpError::Internal(
                    "Too many attempts to find tunnel URL".to_string(),
                ));
            }

            debug!("cloudflared output: {}", line);

            // Look for the tunnel URL
            if let Some(url_match) = url_regex.find(&line) {
                return Ok(url_match.as_str().to_string());
            }

            // Also look for error messages
            if line.contains("error") || line.contains("failed") {
                warn!("Cloudflared error: {}", line);
            }
        }

        Err(HtMcpError::Internal(
            "Could not find tunnel URL in cloudflared output".to_string(),
        ))
    }

    /// Checks if the tunnel process is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Gets the tunnel URL
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Gets the local port
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Stops the tunnel
    pub async fn stop(&mut self) -> Result<()> {
        if self.is_running() {
            info!("Stopping cloudflared tunnel");

            // Try graceful shutdown first
            if let Err(e) = self.child.start_kill() {
                error!("Failed to kill cloudflared process: {}", e);
            }

            // Wait for the process to exit
            match self.child.wait().await {
                Ok(status) => {
                    info!("Cloudflared tunnel stopped with status: {}", status);
                }
                Err(e) => {
                    error!("Error waiting for cloudflared to exit: {}", e);
                }
            }
        }
        Ok(())
    }
}

impl Drop for CloudflareTunnel {
    fn drop(&mut self) {
        if self.is_running() {
            warn!("Cloudflare tunnel being dropped while still running, attempting to kill");
            if let Err(e) = self.child.start_kill() {
                error!("Failed to kill cloudflared process in Drop: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_config_creation() {
        // Test config creation
        let config = TunnelConfig::new(8080);
        assert_eq!(config.port, 8080);
        assert_eq!(config.timeout_secs, Some(30));
        assert_eq!(config.verbose, Some(false));
        assert_eq!(config.provider, Some("cloudflare".to_string()));

        // Test different port
        let config = TunnelConfig::new(3000);
        assert_eq!(config.port, 3000);
    }

    #[tokio::test]
    async fn test_config_builder() {
        let config = TunnelConfig::new(8080)
            .with_timeout(60)
            .with_verbose(true)
            .with_provider("cloudflare".to_string());

        assert_eq!(config.port, 8080);
        assert_eq!(config.timeout_secs, Some(60));
        assert_eq!(config.verbose, Some(true));
        assert_eq!(config.provider, Some("cloudflare".to_string()));
    }

    #[tokio::test]
    async fn test_url_regex() {
        let regex = Regex::new(r"https://[a-zA-Z0-9-]+\.trycloudflare\.com").unwrap();

        let test_line = "2024-01-01T12:00:00Z INF +--------------------------------------------------------------------------------------------+";
        assert!(regex.find(test_line).is_none());

        let test_line_with_url = "2024-01-01T12:00:00Z INF |  Your quick tunnel URL: https://abc123-def456.trycloudflare.com  |";
        assert!(regex.find(test_line_with_url).is_some());

        let found_url = regex.find(test_line_with_url).unwrap().as_str();
        assert_eq!(found_url, "https://abc123-def456.trycloudflare.com");
    }
}
