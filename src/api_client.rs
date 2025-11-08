use reqwest::Client;
use std::error::Error;
use log::{info, debug, error, warn};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use hex;
use tokio::spawn;

/// ------------------ Donate ------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct DonateResponse {
    pub status: Option<String>,
    pub message: Option<String>,
    pub donation_id: Option<String>,
    pub original_address: Option<String>,
    pub destination_address: Option<String>,
    pub timestamp: Option<String>,
    pub solutions_consolidated: Option<u64>,
    pub error: Option<String>,
    pub statusCode: Option<u16>,
}

/// ------------------ Terms & Conditions ------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct TermsResponse {
    pub version: String,
    pub content: String,
    pub message: String,
}

/// ------------------ Register ------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct RegistrationReceipt {
    pub preimage: String,
    pub signature: String,
    pub timestamp: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RegisterResponse {
    #[serde(rename = "registrationReceipt")]
    pub registration_receipt: RegistrationReceipt,
}

/// ------------------ Challenge ------------------
#[derive(Debug, Deserialize, Clone, Serialize)]
pub struct ChallengeParams {
    #[serde(rename = "challenge_id")]
    pub challenge_id: String,
    pub day: Option<u32>,
    #[serde(rename = "challenge_number")]
    pub challenge_number: Option<u32>,
    #[serde(rename = "issued_at")]
    pub issued_at: Option<String>,
    #[serde(rename = "latest_submission")]
    pub latest_submission: Option<String>,
    pub difficulty: Option<String>,
    #[serde(rename = "no_pre_mine")]
    pub no_pre_mine: Option<String>,
    #[serde(rename = "no_pre_mine_hour")]
    pub no_pre_mine_hour: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChallengeResponse {
    pub code: String,
    pub challenge: Option<ChallengeParams>,
    #[serde(rename = "mining_period_ends")]
    pub mining_period_ends: Option<String>,
    #[serde(rename = "max_day")]
    pub max_day: Option<u32>,
    #[serde(rename = "total_challenges")]
    pub total_challenges: Option<u32>,
    #[serde(rename = "current_day")]
    pub current_day: Option<u32>,
    #[serde(rename = "next_challenge_starts_at")]
    pub next_challenge_starts_at: Option<String>,
    #[serde(rename = "starts_at")]
    pub starts_at: Option<String>,
}

/// ------------------ Solution ------------------
#[derive(Debug, Deserialize, Serialize)]
pub struct CryptoReceipt {
    pub preimage: String,
    pub timestamp: String,
    pub signature: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SubmitResponse {
    #[serde(rename = "crypto_receipt")]
    pub crypto_receipt: Option<CryptoReceipt>,
    #[serde(rename = "statusCode")]
    pub status_code: Option<u16>,
    pub message: Option<String>,
}

/// ------------------ ApiClient ------------------
pub struct ApiClient {
    base_url: String,
    http_client: Client,
    backend_url: String,
    backend_token: String
}

impl ApiClient {
    /// Crée un nouveau client API avec timeout raisonnable
    pub fn new(base_url: &str) -> Result<Self, Box<dyn Error>> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()?;

        let backend_url = std::env::var("API_BACKEND_URL")
            .unwrap_or_else(|_| "http://stats-backend:8080/insert_api_return".to_string());
        let backend_token = std::env::var("STATS_BEARER_TOKEN")
            .unwrap_or_else(|_| "secret_token".to_string());


        Ok(Self {
            base_url: base_url.to_string(),
            http_client: client,
            backend_url,
            backend_token
        })
    }

    /// Logging non-bloquant vers le backend
    async fn log_api_call(
        &self,
        container_id: &str,
        miner_id: &str,
        wallet_addr: &str,
        endpoint: &str,
        url: &str,
        description: Option<String>,
        payload: Option<Value>,
        api_response: Option<Value>,
    ) {
        let call_api_enabled = std::env::var("ENABLE_STATS_BACKEND")
            .unwrap_or_else(|_| "false".to_string())
            .to_lowercase() == "true";

        if !call_api_enabled {
            info!("📊 Reporting api désactivé");
            return;
        }            
        let client = self.http_client.clone();
        let token = self.backend_token.clone();
        let miner_id = miner_id.to_string();
        let container_id = container_id.to_string();
        let endpoint = endpoint.to_string();
        let wallet_addr = wallet_addr.to_string();
        let url_ = url.to_string();
        let backend_url = self.backend_url.clone();

        spawn(async move {
            let log_body = serde_json::json!({
                "miner_id": miner_id,
                "container_id": container_id,
                "wallet_addr": wallet_addr,
                "endpoint": endpoint,
                "description": description,
                "payload": payload,
                "url": url_,
                "api_response": api_response,
            });
            match client.post(&backend_url)
                .bearer_auth(token)
                .json(&log_body)
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    info!("✅ Logged API call to backend: endpoint={}", endpoint);
                }
                Ok(resp) => warn!("⚠️ Failed to log API call (status={}): endpoint={}", resp.status(), endpoint),
                Err(e) => warn!("⚠️ Error sending log to backend: endpoint={} err={}", endpoint, e),
            }
        });
    }

    /// Convertit une clé binaire en adresse Bech32
    pub fn to_bech32_address(&self, raw: &[u8]) -> String {
        use bech32::{ToBase32, Variant};
        use sha2::Digest;
        let mut hasher = blake2::Blake2b::<blake2::digest::consts::U28>::new();
        hasher.update(raw);
        let addr_hash = hasher.finalize();
        bech32::encode("addr", addr_hash.to_base32(), Variant::Bech32).unwrap()
    }

    pub async fn get_terms(
        &self,
        version: Option<&str>,
        miner_id: Option<String>,
        container_id: Option<String>
    ) -> Result<TermsResponse, Box<dyn Error + Send + Sync>> {
        let url = version.map_or_else(|| format!("{}/TandC", &self.base_url),
                                      |v| format!("{}/TandC/{}", &self.base_url, v));
        let ua = format!("scavenger_miner/1.0 - github.com/whosbax/midnight-scavenger");

        let resp = self.http_client.get(&url).header("User-Agent", ua).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("GET {} failed [{}]: {}", url, status, text).into());
        }

        let result: TermsResponse = resp.json().await?;
        let api_response_value = Some(
            serde_json::to_value(&result).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?
        );
        self.log_api_call(container_id.as_deref().unwrap_or(""), miner_id.as_deref().unwrap_or(""), "", "/TandC", &url, Some("Fetch terms".to_string()), None, api_response_value).await;
        
        Ok(result)
    }

    pub async fn register_address(
        &self,
        address: &str,
        signature: &str,
        pubkey: &str,
        miner_id: Option<String>,
        container_id: Option<String>
    ) -> Result<RegisterResponse, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/register/{}/{}/{}", &self.base_url, address, signature, pubkey);
        let ua = format!("scavenger_miner/1.0 - github.com/whosbax/midnight-scavenger");

        let resp = self.http_client.post(&url).header("User-Agent", ua).json(&serde_json::json!({})).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        info!("Register addr({}) with pubk({}) -> response: \n{}", address, pubkey, text);
        if !status.is_success() {
            return Err(format!("Registration failed POST[{}] [{}]: {}", url, status, text).into());
        }

        let result: RegisterResponse = serde_json::from_str(&text)?;
        let api_response_value = Some(
            serde_json::to_value(&result).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?
        );
        self.log_api_call(container_id.as_deref().unwrap_or(""), miner_id.as_deref().unwrap_or(""), address.clone(), "/register", &url, Some("Register wallet".to_string()), None, api_response_value).await;
        Ok(result)
    }

    pub async fn get_challenge(&self, miner_id: Option<String>, container_id: Option<String>) -> Result<ChallengeResponse, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/challenge", &self.base_url);
        let ua = format!("scavenger_miner/1.0 - github.com/whosbax/midnight-scavenger");

        let resp = self.http_client.get(&url).header("User-Agent", ua).send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("GET {} failed [{}]: {}", url, status, text).into());
        }

        let result: ChallengeResponse = resp.json().await?;
        let api_response_value = Some(
            serde_json::to_value(&result).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?
        );
        self.log_api_call(container_id.as_deref().unwrap_or(""), miner_id.as_deref().unwrap_or(""), "", "/challenge", &url, Some("Fetch challenge".to_string()), None, api_response_value).await;
        Ok(result)
    }

    pub async fn submit_solution(
        &self,
        address: &str,
        challenge_id: &str,
        nonce: &str,
        preimage: &str,
        miner_id: Option<String>,
        container_id: Option<String>
    ) -> Result<SubmitResponse, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/solution/{}/{}/{}", &self.base_url, address, challenge_id, nonce);
        info!("📬 Soumission de solution addr={} challenge={}", address, challenge_id);
        let ua = format!("scavenger_miner/1.0 - github.com/whosbax/midnight-scavenger");

        let resp = self.http_client.post(&url).header("User-Agent", ua).json(&serde_json::json!({})).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("POST {} failed [{}]: {}", url, status, text).into());
        }

        let result: SubmitResponse = serde_json::from_str(&text)?;
        let api_response_value = Some(
            serde_json::to_value(&result).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?
        );

        self.log_api_call(container_id.as_deref().unwrap_or(""), miner_id.as_deref().unwrap_or(""), address.clone(), "/solution", &url, Some("Submit solution".to_string()), None, api_response_value).await;
        Ok(result)
    }

    pub async fn donate_to(
        &self,
        destination_address: &str,
        original_address: &str,
        signature: &str,
        miner_id: Option<String>,
        container_id: Option<String>
    ) -> Result<DonateResponse, Box<dyn Error + Send + Sync>> {
        let url = format!(
            "{}/donate_to/{}/{}/{}",
            &self.base_url, destination_address, original_address, signature
        );
        info!("💸 Donation Url {}", url);
        let ua = format!("scavenger_miner/1.0 - github.com/whosbax/midnight-scavenger");

        let resp = self.http_client.post(&url).header("User-Agent", ua).json(&serde_json::json!({})).send().await?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            error!("Raw donation response: {}", text);
            return Err(format!("Donation failed [{}]: {}", status, text).into());
        }

        let result: DonateResponse = serde_json::from_str(&text)?;
        let api_response_value = Some(
            serde_json::to_value(&result).map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?
        );

        self.log_api_call(container_id.as_deref().unwrap_or(""), miner_id.as_deref().unwrap_or(""), original_address.clone(), "/donate_to", &url, Some(format!("Donate to {}", destination_address).into()), None, api_response_value).await;
        Ok(result)
    }
}
