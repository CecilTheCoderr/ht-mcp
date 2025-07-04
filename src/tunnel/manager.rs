use crate::error::{HtMcpError, Result};
use crate::tunnel::cloudflare::CloudflareTunnel;
use crate::tunnel::config::TunnelConfig;
use std::collections::HashMap;
use tracing::{error, info, warn};
use uuid::Uuid;

/// Information about an active tunnel
#[derive(Debug, Clone)]
pub struct TunnelInfo {
    pub id: String,
    pub url: String,
    pub local_port: u16,
    pub provider: String,
    pub created_at: std::time::SystemTime,
    pub is_active: bool,
}

/// Manages tunnel instances for the application
pub struct TunnelManager {
    tunnels: HashMap<String, Box<CloudflareTunnel>>,
}

impl TunnelManager {
    pub fn new() -> Self {
        Self {
            tunnels: HashMap::new(),
        }
    }

    /// Creates a simple tunnel for the specified port
    pub async fn create_simple_tunnel(&mut self, port: u16) -> Result<TunnelInfo> {
        let tunnel_id = Uuid::new_v4().to_string();

        info!("Creating cloudflare tunnel on port {}", port);

        let tunnel = CloudflareTunnel::new_simple(port).await?;
        let tunnel_info = TunnelInfo {
            id: tunnel_id.clone(),
            url: tunnel.url().to_string(),
            local_port: tunnel.local_port(),
            provider: "cloudflare".to_string(),
            created_at: std::time::SystemTime::now(),
            is_active: true,
        };

        self.tunnels.insert(tunnel_id, Box::new(tunnel));

        info!(
            "Tunnel created successfully: {} -> {}",
            tunnel_info.local_port, tunnel_info.url
        );
        Ok(tunnel_info)
    }

    /// Creates a new tunnel and returns its information (legacy method)
    pub async fn create_tunnel(&mut self, config: TunnelConfig) -> Result<TunnelInfo> {
        self.create_simple_tunnel(config.port).await
    }

    /// Gets information about a specific tunnel
    pub fn get_tunnel(&self, tunnel_id: &str) -> Option<TunnelInfo> {
        self.tunnels.get(tunnel_id).map(|tunnel| TunnelInfo {
            id: tunnel_id.to_string(),
            url: tunnel.url().to_string(),
            local_port: tunnel.local_port(),
            provider: "cloudflare".to_string(), // Currently only cloudflare
            created_at: std::time::SystemTime::now(), // TODO: Store actual creation time
            is_active: true,                    // TODO: Check actual status
        })
    }

    /// Lists all active tunnels
    pub fn list_tunnels(&self) -> Vec<TunnelInfo> {
        self.tunnels
            .iter()
            .map(|(id, tunnel)| TunnelInfo {
                id: id.clone(),
                url: tunnel.url().to_string(),
                local_port: tunnel.local_port(),
                provider: "cloudflare".to_string(),
                created_at: std::time::SystemTime::now(), // TODO: Store actual creation time
                is_active: true,                          // TODO: Check actual status
            })
            .collect()
    }

    /// Stops and removes a tunnel
    pub async fn stop_tunnel(&mut self, tunnel_id: &str) -> Result<()> {
        if let Some(mut tunnel) = self.tunnels.remove(tunnel_id) {
            info!("Stopping tunnel: {}", tunnel_id);
            tunnel.stop().await?;
            info!("Tunnel stopped: {}", tunnel_id);
            Ok(())
        } else {
            Err(HtMcpError::Internal(format!(
                "Tunnel not found: {}",
                tunnel_id
            )))
        }
    }

    /// Stops all tunnels
    pub async fn stop_all_tunnels(&mut self) -> Result<()> {
        info!("Stopping all tunnels");
        let tunnel_ids: Vec<String> = self.tunnels.keys().cloned().collect();

        for tunnel_id in tunnel_ids {
            if let Err(e) = self.stop_tunnel(&tunnel_id).await {
                error!("Failed to stop tunnel {}: {}", tunnel_id, e);
            }
        }

        info!("All tunnels stopped");
        Ok(())
    }

    /// Checks the health of all tunnels and removes dead ones
    pub async fn health_check(&mut self) -> Result<()> {
        let mut dead_tunnels = Vec::new();

        for (id, tunnel) in self.tunnels.iter_mut() {
            if !tunnel.is_running() {
                warn!("Tunnel {} is no longer running", id);
                dead_tunnels.push(id.clone());
            }
        }

        for id in dead_tunnels {
            self.tunnels.remove(&id);
            info!("Removed dead tunnel: {}", id);
        }

        Ok(())
    }

    /// Gets the number of active tunnels
    pub fn tunnel_count(&self) -> usize {
        self.tunnels.len()
    }
}

impl Drop for TunnelManager {
    fn drop(&mut self) {
        if !self.tunnels.is_empty() {
            warn!(
                "TunnelManager being dropped with {} active tunnels",
                self.tunnels.len()
            );
            // The individual tunnels will be dropped and cleaned up automatically
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tunnel_manager_creation() {
        let manager = TunnelManager::new();
        assert_eq!(manager.tunnel_count(), 0);
        assert!(manager.list_tunnels().is_empty());
    }

    #[tokio::test]
    #[ignore] // Skip this test since it requires cloudflared to be installed
    async fn test_tunnel_manager_simple_creation() {
        let mut manager = TunnelManager::new();

        // Test the simple tunnel creation method (would fail without cloudflared installed)
        let result = manager.create_simple_tunnel(8080).await;
        // We expect this to fail since cloudflared is not installed in test environment
        assert!(result.is_err());

        if let Err(HtMcpError::Internal(msg)) = result {
            assert!(msg.contains("Failed to spawn cloudflared"));
        }
    }

    #[tokio::test]
    async fn test_tunnel_info_structure() {
        let info = TunnelInfo {
            id: "test-id".to_string(),
            url: "https://test.trycloudflare.com".to_string(),
            local_port: 8080,
            provider: "cloudflare".to_string(),
            created_at: std::time::SystemTime::now(),
            is_active: true,
        };

        assert_eq!(info.id, "test-id");
        assert_eq!(info.url, "https://test.trycloudflare.com");
        assert_eq!(info.local_port, 8080);
        assert_eq!(info.provider, "cloudflare");
        assert!(info.is_active);
    }
}
