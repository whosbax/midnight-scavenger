use axum::{
    extract::{Json, State},
    http::HeaderMap,
    routing::post,
    Router,
};
use chrono::{DateTime, Utc, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Postgres, postgres::PgPoolOptions};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error};
use tracing_subscriber;
use sqlx::types::Json as sqlxJson;

// -------------------- STRUCTURES --------------------

#[derive(Debug, Serialize, Deserialize)]
struct Stat {
    container_id: String,
    miner_id: String,
    hash_rate: f64,
    timestamp: DateTime<Utc>,
    description: Option<String>
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiReturn {
    container_id: String,
    miner_id: String,
    wallet_addr: Option<String>,
    url: String,
    endpoint: String,
    description: Option<String>,
    payload: Option<Value>,
    api_response: Option<Value>,
}

// -------------------- HELPERS --------------------

fn get_bearer_token() -> String {
    std::env::var("STATS_BEARER_TOKEN")
        .unwrap_or_else(|_| "changeme".to_string())
}

// Vérifie le header Authorization Bearer
fn check_bearer(headers: &HeaderMap) -> bool {
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            let expected = format!("Bearer {}", get_bearer_token());
            return auth_str == expected;
        }
    }
    false
}

// -------------------- HANDLERS --------------------

#[axum::debug_handler]
async fn insert_stat(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(payload): Json<Stat>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    if !check_bearer(&headers) {
        return Err(Json(serde_json::json!({"status": "error", "message": "Unauthorized"})));
    }

    info!("📥 Received stat: miner_id={} hash_rate={} timestamp={}", 
        payload.miner_id, payload.hash_rate, payload.timestamp);

    let ts_naive: NaiveDateTime = payload.timestamp.naive_utc();

    match sqlx::query(
        "INSERT INTO stats (container_id, miner_id, hash_rate, timestamp) VALUES ($1, $2, $3, $4)"
    ) 
    .bind(&payload.container_id)
    .bind(&payload.miner_id)
    .bind(payload.hash_rate)
    .bind(ts_naive)
    .execute(&pool)
    .await
    {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => {
            error!("❌ DB insert error: {:?}", e);
            Err(Json(serde_json::json!({"status": "error", "message": e.to_string()})))
        }
    }
}

#[axum::debug_handler]
async fn insert_api_return(
    State(pool): State<Pool<Postgres>>,
    headers: HeaderMap,
    Json(payload): Json<ApiReturn>,
) -> Result<Json<serde_json::Value>, Json<serde_json::Value>> {
    if !check_bearer(&headers) {
        return Err(Json(serde_json::json!({"status": "error", "message": "Unauthorized"})));
    }

    info!("📥 Received API return: miner_id={} endpoint={}", payload.miner_id, payload.endpoint);

    let res = sqlx::query(
        "INSERT INTO api_return (container_id, miner_id, wallet_addr, timestamp, url, endpoint, description, payload, api_response)
        VALUES ($1, $2, $3, NOW(), $4, $5, $6, $7, $8)"
    )
    .bind(&payload.container_id)
    .bind(&payload.miner_id)
    .bind(&payload.wallet_addr)
    .bind(&payload.url)
    .bind(&payload.endpoint)
    .bind(&payload.description)
    .bind(payload.payload.map(sqlxJson))       
    .bind(payload.api_response.map(sqlxJson))  
    .execute(&pool)
    .await;

    match res {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => {
            error!("❌ Failed to log API return: {:?}", e);
            Err(Json(serde_json::json!({"status": "error", "message": e.to_string()})))
        }
    }
}

// -------------------- MAIN --------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("🚀 Starting stats-backend...");

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://stats:stats_pass@stats-db:5432/stats".to_string());
    info!("Using DATABASE_URL={}", database_url);

    let pool = loop {
        match PgPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&database_url)
            .await
        {
            Ok(pool) => break pool,
            Err(e) => {
                warn!("⏳ Waiting for Postgres... ({})", e);
                sleep(Duration::from_secs(3)).await;
            }
        }
    };

    let app = Router::new()
        .route("/insert_stat", post(insert_stat))
        .route("/insert_api_return", post(insert_api_return))
        .with_state(pool.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080)); 
    info!("🌍 Listening on http://{}", addr); 
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;


    Ok(())
}
