
use alloy::providers::ProviderBuilder;
use monad_bot::validators::{FilterConfig, TokenAnalyzer}; // Crate name matches Cargo.toml? "monad-bot" usually creates lib name "monad_bot" (underscore). Checking...
use std::sync::Arc;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Setup RPC
    let rpc_url = "https://practical-neat-telescope.monad-mainnet.quiknode.pro/730346a87672e9b4d50429263f445f1192e7ca71/";
    let provider = ProviderBuilder::new().on_http(rpc_url.parse()?);

    println!("✅ Connected to RPC");

    // 2. Setup Analyzer
    let config = FilterConfig::default();
    let analyzer = TokenAnalyzer::new(provider, config, 0.50);

    // 3. Analyze WMON (Known Token)
    let wmon_addr = "0x760AfE86e5de5fa0Ee542fc7B7B713e1c5425701".parse()?;
    
    println!("🔍 Analyzing WMON: {:?}", wmon_addr);

    // Simulate analysis (liquidity 1000 MON, age big)
    let analysis = analyzer.analyze(
        wmon_addr,
        None, // No dev wallet known
        0,    // Very old timestamp
        1000.0 // 1000 MON liquidity
    ).await;

    println!("📊 Result:");
    println!("   Safe: {}", analysis.is_safe);
    println!("   Reason: {:?}", analysis.rejection_reason);
    println!("   Data: Supply={}, Mcap=${:.2}", analysis.total_supply, analysis.market_cap_usd);

    if analysis.total_supply > alloy::primitives::U256::ZERO {
        println!("✅ RPC Check Passed (Total Supply fetched)");
    } else {
        println!("❌ RPC Check Failed");
    }

    Ok(())
}
