use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfig {
    /// The local port to tunnel
    pub port: u16,

    /// Optional path to the cloudflared binary (legacy support)
    pub bin_path: Option<PathBuf>,

    /// Timeout for tunnel startup in seconds (legacy support)
    pub timeout_secs: Option<u64>,

    /// Enable verbose logging (legacy support)
    pub verbose: Option<bool>,

    /// Tunnel provider (legacy support)
    pub provider: Option<String>,

    /// Authentication token for the tunnel service (legacy support)
    pub auth_token: Option<String>,

    /// Custom domain for the tunnel (legacy support)
    pub custom_domain: Option<String>,
}

impl TunnelConfig {
    pub fn new(port: u16) -> Self {
        Self {
            port,
            bin_path: None,
            timeout_secs: Some(30),
            verbose: Some(false),
            provider: Some("cloudflare".to_string()),
            auth_token: None,
            custom_domain: None,
        }
    }

    pub fn with_bin_path(mut self, bin_path: PathBuf) -> Self {
        self.bin_path = Some(bin_path);
        self
    }

    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = Some(verbose);
        self
    }

    pub fn with_provider(mut self, provider: String) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn with_auth_token(mut self, auth_token: String) -> Self {
        self.auth_token = Some(auth_token);
        self
    }

    pub fn with_custom_domain(mut self, custom_domain: String) -> Self {
        self.custom_domain = Some(custom_domain);
        self
    }
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self::new(8080)
    }
}
