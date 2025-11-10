// src/stats_client.rs
use std::sync::Arc;
use std::sync::atomic::{AtomicU64};
use tokio::time::{interval, Duration, Instant};
use serde::Serialize;
use reqwest::Client;
use log::{info, warn};
use chrono::Utc;

#[derive(Serialize)]
struct StatsPayload<'a> {
    container_id: String,
    miner_id: &'a str,
    timestamp: String,
    hash_rate: f64,
    uptime_secs: u64,
    version: &'a str,
}

/// Lancement du reporter de stats
pub fn start_stats_reporter(
    container_id: String,
    miner_id: String,
    hash_counter: Arc<AtomicU64>,
    server_url: String,
    version: String,
    report_interval_secs: u64,
) {

    let client = Client::builder()
        .pool_idle_timeout(Duration::from_secs(15))
        .build()
        .expect("Failed to create HTTP client");

    let bearer_token = std::env::var("STATS_BEARER_TOKEN").unwrap_or_default();
    let ctn_prefix = std::env::var("CONTAINER_PREFIX").unwrap_or_else(|_| "".to_string());

    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(report_interval_secs));
        let mut last_instant = Instant::now();
        let start_time = Utc::now();

        loop {
            ticker.tick().await;

            // Mesure claire de l'intervalle écoulé entre deux ticks
            let now = Instant::now();
            let elapsed = now.duration_since(last_instant).as_secs_f64();
            last_instant = now;

            // Lecture atomique & remise à zéro
            let hashes = hash_counter.swap(0, std::sync::atomic::Ordering::AcqRel) as f64;
            if hashes == 0.0 {
                info!("Aucun hash calculé depuis le dernier tick");
                continue; 
            }

            let hashrate = if elapsed > 0.0 { hashes / elapsed } else { 0.0 };
            let uptime = (Utc::now() - start_time).num_seconds().max(0) as u64;
            //let ctn_id = format!("{}", ctn_prefix);
            let ctn_id = format!("{}/{}", ctn_prefix, container_id.clone());
            let payload = StatsPayload {
                container_id: ctn_id.clone(),
                miner_id: &miner_id,
                timestamp: Utc::now().to_rfc3339(),
                hash_rate: hashrate,
                uptime_secs: uptime,
                version: &version,
            };
            info!(
                "📥  stat: miner_id={} hash_rate={} timestamp={}",
                payload.miner_id,
                payload.hash_rate,
                payload.timestamp
            );

            let call_api_enabled = std::env::var("ENABLE_STATS_BACKEND")
                .unwrap_or_else(|_| "false".to_string())
                .to_lowercase() == "true";
            
            if !call_api_enabled {                
                info!("📊 Reporting hash rate désactivé");
                return;
            } 
            let body = match serde_json::to_vec(&payload) {
                Ok(b) => b,
                Err(e) => {
                    warn!("Failed to serialize stats payload: {}", e);
                    continue;
                }
            };

            let url = server_url.clone();
            let client = client.clone();
            let bearer_token = bearer_token.clone();

            // Fire-and-forget, timeout très court
            tokio::spawn(async move {
                let req = client.post(&url)
                    .header("content-type", "application/json")
                    .header("Authorization", format!("Bearer {}", bearer_token))
                    .body(body);

                match tokio::time::timeout(Duration::from_secs(1), req.send()).await {
                    Ok(Ok(resp)) => {
                        if !resp.status().is_success() {
                            warn!("Stats sent but server returned status={}", resp.status());
                        } else {
                            info!("Stats sent successfully ({} H/s)", hashrate);
                        }
                    }
                    Ok(Err(e)) => warn!("HTTP error sending stats: {}", e),
                    Err(_) => warn!("Stats send timed out"),
                }
            });
        }
    });
}
