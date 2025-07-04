#!/usr/bin/env rust-script
//! Cloudflared Tunnel Demo
//!
//! This example demonstrates how to use the cloudflared tunnel functionality
//! with the HT-MCP server. It shows how to:
//!
//! 1. Create a session with both web server and tunnel enabled
//! 2. Create standalone tunnels
//! 3. List and manage tunnels
//!
//! To run this example (requires cloudflared to be installed):
//! ```bash
//! cargo run --example tunnel_demo
//! ```

use ht_mcp::{TunnelConfig, TunnelManager};
use tokio;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info,ht_mcp=debug")
        .init();

    info!("🚀 Cloudflared Tunnel Demo Starting");

    // Create a tunnel manager
    let mut tunnel_manager = TunnelManager::new();

    println!("\n📋 Step 1: Creating a tunnel for port 8080");

    // Create a tunnel configuration
    let config = TunnelConfig::new(8080).with_timeout(30).with_verbose(true);

    // Configuration is always valid in simplified implementation
    info!("✅ Tunnel configuration created");

    // Note: This would actually create a tunnel if cloudflared is installed
    // For this demo, we'll show what the API looks like
    println!("🔧 Configuration created:");
    println!("   Port: {}", config.port);
    println!("   Timeout: {:?}s", config.timeout_secs);
    println!("   Verbose: {:?}", config.verbose);
    println!("   Provider: {:?}", config.provider);

    println!("\n📋 Step 2: Simulate tunnel creation");

    // In a real scenario with cloudflared installed, this would work:
    // match tunnel_manager.create_tunnel(config).await {
    //     Ok(tunnel_info) => {
    //         info!("✅ Tunnel created successfully!");
    //         println!("   Tunnel ID: {}", tunnel_info.id);
    //         println!("   Tunnel URL: {}", tunnel_info.url);
    //         println!("   Local Port: {}", tunnel_info.local_port);
    //         println!("   Provider: {}", tunnel_info.provider);
    //     }
    //     Err(e) => {
    //         error!("❌ Failed to create tunnel: {}", e);
    //         println!("   This is expected if cloudflared is not installed");
    //     }
    // }

    println!("   📝 Note: Actual tunnel creation requires cloudflared binary");
    println!("   📥 Install from: https://github.com/cloudflare/cloudflared/releases");

    println!("\n📋 Step 3: Simple configuration examples");

    // Test various configurations
    let configs = vec![
        ("Port 3000", TunnelConfig::new(3000)),
        ("Port 8080", TunnelConfig::new(8080)),
        ("Port 65535", TunnelConfig::new(65535)),
    ];

    for (description, config) in configs {
        println!("   ✅ {}: Created with port {}", description, config.port);
    }

    println!("\n📋 Step 4: Builder pattern example");

    let advanced_config = TunnelConfig::new(9000)
        .with_timeout(60)
        .with_verbose(true)
        .with_provider("cloudflare".to_string())
        .with_auth_token("your-token-here".to_string());

    println!("   Advanced configuration created:");
    println!("   Port: {}", advanced_config.port);
    println!("   Timeout: {:?}s", advanced_config.timeout_secs);
    println!("   Verbose: {:?}", advanced_config.verbose);
    println!("   Provider: {:?}", advanced_config.provider);
    println!(
        "   Has Auth Token: {}",
        advanced_config.auth_token.is_some()
    );

    println!("\n📋 Step 5: Tunnel manager features");

    // Show tunnel manager capabilities
    let tunnel_count = tunnel_manager.tunnel_count();
    println!("   Current tunnel count: {}", tunnel_count);

    let tunnels = tunnel_manager.list_tunnels();
    println!("   Active tunnels: {}", tunnels.len());

    println!("\n✨ Demo completed successfully!");
    println!("\n🔗 Integration with HT-MCP:");
    println!("   • Use 'ht_create_session' with 'enableTunnel: true'");
    println!("   • Tunnel automatically uses the web server port");
    println!("   • Simple command: cloudflared tunnel --url http://localhost:PORT");

    println!("\n📖 Example MCP tool call:");
    println!(
        r#"   {{
     "name": "ht_create_session",
     "arguments": {{
       "enableWebServer": true,
       "enableTunnel": true
     }}
   }}"#
    );

    println!("\n💡 How it works:");
    println!("   1. enableWebServer: true starts HT web server on available port");
    println!("   2. enableTunnel: true runs: cloudflared tunnel --url http://localhost:PORT");
    println!("   3. Returns both webServerUrl and tunnelUrl in the response");

    Ok(())
}
