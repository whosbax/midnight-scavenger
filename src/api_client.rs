// src/api_client.rs

use reqwest::Client;
use std::error::Error;
use log::{info, debug, error};
use serde::{Deserialize, Serialize};

/// ------------------ Donate ------------------
    #[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
pub struct TermsResponse {
    pub version: String,
    pub content: String,
    pub message: String,
}

/// ------------------ Register ------------------
#[derive(Debug, Deserialize)]
pub struct RegistrationReceipt {
    pub preimage: String,
    pub signature: String,
    pub timestamp: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterResponse {
    #[serde(rename = "registrationReceipt")]
    pub registration_receipt: RegistrationReceipt,
}

/// ------------------ Challenge ------------------
#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize)]
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
#[derive(Debug, Deserialize)]
pub struct CryptoReceipt {
    pub preimage: String,
    pub timestamp: String,
    pub signature: String,
}

#[derive(Debug, Deserialize)]
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
}

impl ApiClient {
    pub fn new(base_url: &str) -> Result<Self, Box<dyn Error>> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()?;
        Ok(Self {
            base_url: base_url.to_string(),
            http_client: client,
        })
    }
    pub fn to_bech32_address(&self, raw: &[u8]) -> String {
        use bech32::{ToBase32, Variant};
        use sha2::{Digest};

        // Cardano uses blake2b224 for payment key hash (28 bytes)
        let mut hasher = blake2::Blake2b::<blake2::digest::consts::U28>::new();
        hasher.update(raw);
        let addr_hash = hasher.finalize();

        // prefix 'addr' for mainnet, 'addr_test' for testnet
        bech32::encode("addr", addr_hash.to_base32(), Variant::Bech32).unwrap()
    }
    /// GET /TandC[/{version}]
    pub async fn get_terms(&self, version: Option<&str>) -> Result<TermsResponse, Box<dyn Error>> {
        let url = match version {
            Some(ver) => format!("{}/TandC/{}", &self.base_url, ver),
            None => format!("{}/TandC", &self.base_url),
        };
        let resp = self.http_client.get(&url).header("User-Agent", "scavenger_miner/1.0 - https://github.com/whosbax/midnight-scavenger").send().await?;
        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("GET {} failed: {}", url, text).into());
        }
        Ok(resp.json().await?)
    }

    /// POST /register/{address}/{signature}/{pubkey}
    pub async fn register_address(
        &self,
        address: &str,
        signature: &str,
        pubkey: &str,
    ) -> Result<RegisterResponse, Box<dyn Error>> {
        let url = format!("{}/register/{}/{}/{}", &self.base_url, address, signature, pubkey);
        let resp = self.http_client.post(&url).header("User-Agent", "scavenger_miner/1.0 - https://github.com/whosbax/midnight-scavenger").json(&serde_json::json!({})).send().await?;
        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("POST {} failed: {}", url, text).into());
        }
        Ok(resp.json().await?)
    }

    /// GET /challenge
    pub async fn get_challenge(&self) -> Result<ChallengeResponse, Box<dyn Error>> {
        let url = format!("{}/challenge", &self.base_url);
        let resp = self.http_client.get(&url).header("User-Agent", "scavenger_miner/1.0 - https://github.com/whosbax/midnight-scavenger").send().await?;
        if !resp.status().is_success() {
            let text = resp.text().await?;
            return Err(format!("GET {} failed: {}", url, text).into());
        }
        Ok(resp.json().await?)
    }

    /// POST /solution/{address}/{challenge_id}/{nonce}
    pub async fn submit_solution(
        &self,
        address: &str,
        challenge_id: &str,
        nonce: &str,
        preimage: &str
    ) -> Result<SubmitResponse, Box<dyn Error>> {
        let url = format!("{}/solution/{}/{}/{}", &self.base_url, address, challenge_id, nonce);
        
        info!("📬 Submitting solution");
        info!("  URL        : {}", url);
        info!("  Address    : {}", address);
        info!("  Challenge  : {}", challenge_id);
        info!("  Nonce      : {}", nonce);
        info!("  Preimage   : {}", preimage);

        let resp = self
            .http_client
            .post(&url)
            .header("User-Agent", "scavenger_miner/1.0 - https://github.com/whosbax/midnight-scavenger")
            .json(&serde_json::json!({})) // body vide
            .send()
            .await?;

        if !resp.status().is_success() {
            let text = resp.text().await?;
            error!("❌ POST {} failed: {}", url, text);
            return Err(format!("POST {} failed: {}", url, text).into());
        }

        let submit_resp = resp.json::<SubmitResponse>().await?;
        info!("✅ Submission response: {:?}", submit_resp);

        Ok(submit_resp)
    }




    pub async fn donate_to(
        &self,
        base_url: &str,
        destination_address: &str,
        original_address: &str,
        signature: &str,
    ) -> Result<DonateResponse, String> {
        let url = format!(
            "{}/donate_to/{}/{}/{}",
            base_url, destination_address, original_address, signature
        );

        info!("💸 Submitting donation assignment →");
        info!("  URL        : {}", url);
        info!("  From (original): {}", original_address);
        info!("  To   (dest)   : {}", destination_address);

        let resp = self
            .http_client
            .post(&url)
            .header("User-Agent", "scavenger_miner/1.0 - https://github.com/whosbax/midnight-scavenger")
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;

        debug!("Raw response: {}", text);

        if !status.is_success() {
            error!("❌ Donation failed [{}]: {}", status, text);
            return Err(format!("Donation failed: {}", text));
        }

        serde_json::from_str::<DonateResponse>(&text)
            .map_err(|e| format!("JSON parse error: {} / raw={}", e, text))
    }

}
