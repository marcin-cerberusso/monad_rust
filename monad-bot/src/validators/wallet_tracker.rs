use alloy::primitives::Address;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use tracing::{info, warn};

const WALLET_STATS_FILE: &str = "wallet_stats.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletStats {
    pub total_trades: u32,
    pub wins: u32,
    pub losses: u32,
    
    // Performance Metrics
    pub total_pnl_mon: f64,      // Net profit taking losses into account
    pub total_invested_mon: f64, // Volume traded
    pub avg_roi_pct: f64,        // Average Return on Investment per trade
    
    // Timing
    pub avg_hold_time_sec: u64,
    pub last_trade_time: u64,
    
    // Advanced
    pub win_streak: u32,
    pub best_trade_mon: f64,
    pub worst_trade_mon: f64,
}

impl Default for WalletStats {
    fn default() -> Self {
        Self {
            total_trades: 0,
            wins: 0,
            losses: 0,
            total_pnl_mon: 0.0,
            total_invested_mon: 0.0,
            avg_roi_pct: 0.0,
            avg_hold_time_sec: 0,
            last_trade_time: 0,
            win_streak: 0,
            best_trade_mon: 0.0,
            worst_trade_mon: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionEntry {
    pub entry_price_mon: f64, // Total MON spent
    pub entry_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletTracker {
    pub stats: HashMap<Address, WalletStats>,
    
    // Track active positions with timestamp
    // Map<Wallet, Map<Token, PositionEntry>>
    #[serde(skip)]
    pub active_positions: HashMap<Address, HashMap<Address, PositionEntry>>,
}

impl WalletTracker {
    pub fn load() -> Self {
        let content = fs::read_to_string(WALLET_STATS_FILE).unwrap_or_else(|_| "{}".to_string());
        // Simple migration check: if json structure changed drastically, start fresh or handle error
        // For now, we assume fresh start if deserialization fails
        let stats: HashMap<Address, WalletStats> = serde_json::from_str(&content).unwrap_or_default();
        
        Self {
            stats,
            active_positions: HashMap::new(),
        }
    }

    pub fn save(&self) {
        let json = serde_json::to_string_pretty(&self.stats).unwrap_or_default();
        if let Err(e) = fs::write(WALLET_STATS_FILE, json) {
            warn!("Failed to save wallet stats: {}", e);
        }
    }

    pub fn record_buy(&mut self, wallet: Address, token: Address, entry_price_mon: f64) {
        let entry = PositionEntry {
            entry_price_mon,
            entry_time: chrono::Utc::now().timestamp() as u64,
        };
        
        self.active_positions
            .entry(wallet)
            .or_default()
            .insert(token, entry);
    }

    pub fn record_sell(&mut self, wallet: Address, token: Address, exit_price_mon: f64) -> Option<f64> {
        let entry_data = self.active_positions
            .get_mut(&wallet)
            .and_then(|tokens| tokens.remove(&token));

        if let Some(entry) = entry_data {
            let pnl = exit_price_mon - entry.entry_price_mon;
            // ROI = (PnL / Invested) * 100
            let roi = if entry.entry_price_mon > 0.0 {
                (pnl / entry.entry_price_mon) * 100.0
            } else {
                0.0
            };
            
            let now = chrono::Utc::now().timestamp() as u64;
            let hold_time = now.saturating_sub(entry.entry_time);

            let stats = self.stats.entry(wallet).or_default();
            
            // Update counts
            stats.total_trades += 1;
            stats.last_trade_time = now;
            stats.total_invested_mon += entry.entry_price_mon;
            stats.total_pnl_mon += pnl;

            // Updating averages (simple moving average approximation)
            // New Avg = ((Old Avg * (N-1)) + New Val) / N
            if stats.total_trades > 1 {
                stats.avg_roi_pct = ((stats.avg_roi_pct * (stats.total_trades as f64 - 1.0)) + roi) / stats.total_trades as f64;
                stats.avg_hold_time_sec = ((stats.avg_hold_time_sec * (stats.total_trades as u64 - 1)) + hold_time) / stats.total_trades as u64;
            } else {
                stats.avg_roi_pct = roi;
                stats.avg_hold_time_sec = hold_time;
            }

            // Win/Loss stats
            if pnl > 0.0 {
                stats.wins += 1;
                stats.win_streak += 1;
            } else {
                stats.losses += 1;
                stats.win_streak = 0;
            }
            
            // Records
            if pnl > stats.best_trade_mon {
                stats.best_trade_mon = pnl;
            }
            if pnl < stats.worst_trade_mon {
                stats.worst_trade_mon = pnl;
            }

            info!(
                "📊 Tracker Update: {:?} | PnL: {:.4} MON | ROI: {:.2}% | Hold: {}s | Score: {:.1}",
                wallet, pnl, roi, hold_time, Self::calculate_score(stats)
            );

            self.save();
            return Some(pnl);
        }
        
        None
    }

    /// The "Golden Score" Algorithm
    /// Returns 0.0 - 100.0 describing wallet quality
    pub fn get_score(&self, wallet: &Address) -> f64 {
        if let Some(stats) = self.stats.get(wallet) {
            Self::calculate_score(stats)
        } else {
            50.0 // Neutral start
        }
    }
    
    fn calculate_score(stats: &WalletStats) -> f64 {
        if stats.total_trades < 3 {
             return 50.0; // Needs data
        }

        // 1. Win Rate Score (0-40 pts)
        let win_rate = stats.wins as f64 / stats.total_trades as f64;
        let score_wr = win_rate * 40.0;

        // 2. ROI Factor (0-30 pts)
        // Avg ROI of 10% = good (15 pts), 50% = amazing (30 pts)
        let score_roi = (stats.avg_roi_pct * 0.6).clamp(-10.0, 30.0);

        // 3. PnL Factor (0-20 pts)
        // 10 MON profit = 10 pts, 20 MON = 20 pts (max)
        let score_pnl = stats.total_pnl_mon.clamp(-20.0, 20.0);

        // 4. Streak Bonus (0-10 pts)
        // 3 wins in a row = 3 pts
        let score_streak = (stats.win_streak as f64).min(10.0);

        let total_score = score_wr + score_roi + score_pnl + score_streak;
        
        // Clamp final result 0-100
        total_score.clamp(0.0, 100.0)
    }
}
