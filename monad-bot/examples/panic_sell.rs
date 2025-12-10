use alloy::primitives::{Address, U256};
use nadfun_sdk::Core;
use nadfun_sdk::Network;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;
use alloy::sol;
use nadfun_sdk::types::{SellParams, Router};

// ERC20 interface
sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
        function allowance(address owner, address spender) external view returns (uint256);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub token: Address,
    pub name: String,
    pub symbol: String,
    pub amount: U256,
    pub buy_price_mon: f64,
    pub buy_time: u64,
    pub highest_price: f64,
    pub tx_hash: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    
    let rpc_url = std::env::var("MONAD_RPC_URL").expect("MONAD_RPC_URL not set");
    let private_key = std::env::var("PRIVATE_KEY").expect("PRIVATE_KEY not set");

    let core = Core::new(rpc_url, private_key, Network::Mainnet).await?;
    let wallet = core.wallet_address();
    println!("🔥 PANIC SELL MODE ACTIVATED for wallet {:?}", wallet);

    // Load positions
    let content = fs::read_to_string("positions.json").unwrap_or_else(|_| "{}".to_string());
    let positions: HashMap<Address, Position> = serde_json::from_str(&content).unwrap_or_default();

    println!("📊 Found {} positions to sell", positions.len());

    for (token, pos) in positions {
        println!("Selling {} ({:?})...", pos.name, token);

        // Check actual balance
        let token_contract = IERC20::new(token, core.provider());
        // Alloy 0.9.x returns simple type for single value return
        let balance = token_contract.balanceOf(wallet).call().await?; 

        if balance == U256::ZERO {
            println!("⚠️ Balance is 0 for {}, skipping...", pos.name);
            continue;
        }

        println!("💰 Balance: {}", balance);

        // Get Router via get_amount_out to be sure
        println!("🔍 Fetching quote to find router...");
        let (router, _) = core.get_amount_out(token, balance, false).await.unwrap_or_else(|_| {
            // Fallback to default bonding curve address from env if quote fails
            let router_env = std::env::var("ROUTER_ADDRESS").unwrap_or("0x6F6B8F1a20703309951a5127c45B49b1CD981A22".to_string());
            let r_addr = Address::from_str(&router_env).unwrap();
            (Router::BondingCurve(r_addr), U256::ZERO)
        });
        
        let router_address = router.address();
        println!("🔄 Using router: {:?}", router);

        let allowance = token_contract.allowance(wallet, router_address).call().await?;
        if allowance < balance {
            println!("🔓 Approving tokens...");
            let tx = token_contract.approve(router_address, U256::MAX).send().await?.watch().await?;
            println!("✅ Approved: {:?}", tx);
        }

        // Sell
        let sell_params = SellParams {
            token,
            amount_in: balance,
            amount_out_min: U256::ZERO, // 100% Slippage
            to: wallet,
            deadline: U256::from(9999999999999999u64),
            gas_limit: Some(500000),
            gas_price: None,
            nonce: None,
        };

        match core.sell(sell_params, router).await {
            Ok(hash) => println!("✅ SOLD! Tx: {}", hash),
            Err(e) => println!("❌ Sell failed: {}", e),
        }
    }
    
    // Clear positions file
    fs::write("positions.json", "{}")?;
    println!("🧹 Positions file cleared.");

    Ok(())
}
