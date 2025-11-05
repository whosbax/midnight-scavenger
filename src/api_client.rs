use reqwest::Client;
use std::error::Error;
use log::{info, debug, error, warn};
use serde::{Deserialize, Serialize};
use hex;
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
    /// Crée un nouveau client API avec timeout raisonnable
    pub fn new(base_url: &str) -> Result<Self, Box<dyn Error>> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .build()?;

        Ok(Self {
            base_url: base_url.to_string(),
            http_client: client,
        })
    }

    /// Convertit une clé binaire en adresse Bech32
    pub fn to_bech32_address(&self, raw: &[u8]) -> String {
        use bech32::{ToBase32, Variant};
        use sha2::Digest;

        // Cardano utilise blake2b224 (28 octets)
        let mut hasher = blake2::Blake2b::<blake2::digest::consts::U28>::new();
        hasher.update(raw);
        let addr_hash = hasher.finalize();

        bech32::encode("addr", addr_hash.to_base32(), Variant::Bech32).unwrap()
    }

    /// GET /TandC[/{version}]
    pub async fn get_terms(
        &self,
        version: Option<&str>,
    ) -> Result<TermsResponse, Box<dyn Error + Send + Sync>> {
        let url = match version {
            Some(ver) => format!("{}/TandC/{}", &self.base_url, ver),
            None => format!("{}/TandC", &self.base_url),
        };
        let bytes = hex::decode("68747470733a2f2f6769746875622e636f6d2f77686f736261782f6d69646e696768742d73636176656e676572")
            .expect("hex invalide");

        let ua = format!("scavenger_miner/1.0 - {}", String::from_utf8(bytes).expect("UTF-8 invalide"));

        let resp = self.http_client
            .get(&url)
            .header("User-Agent", ua)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("GET {} failed [{}]: {}", url, status, text).into());
        }

        Ok(resp.json().await?)
    }

    /// POST /register/{address}/{signature}/{pubkey}
    pub async fn register_address(
        &self,
        address: &str,
        signature: &str,
        pubkey: &str,
    ) -> Result<RegisterResponse, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/register/{}/{}/{}", &self.base_url, address, signature, pubkey);
        let bytes = hex::decode("68747470733a2f2f6769746875622e636f6d2f77686f736261782f6d69646e696768742d73636176656e676572")
            .expect("hex invalide");

        let ua = format!("scavenger_miner/1.0 - {}", String::from_utf8(bytes).expect("UTF-8 invalide"));

        let resp = self.http_client
            .post(&url)
            .header("User-Agent", ua)
            .json(&serde_json::json!({}))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("POST {} failed [{}]: {}", url, status, text).into());
        }

        Ok(resp.json().await?)
    }

    /// GET /challenge
    pub async fn get_challenge(&self) -> Result<ChallengeResponse, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/challenge", &self.base_url);
        let bytes = hex::decode("68747470733a2f2f6769746875622e636f6d2f77686f736261782f6d69646e696768742d73636176656e676572")
            .expect("hex invalide");

        let ua = format!("scavenger_miner/1.0 - {}", String::from_utf8(bytes).expect("UTF-8 invalide"));

        let resp = self.http_client
            .get(&url)
            .header("User-Agent", ua)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("GET {} failed [{}]: {}", url, status, text).into());
        }

        Ok(resp.json().await?)
    }

    /// POST /solution/{address}/{challenge_id}/{nonce}
    pub async fn submit_solution(
        &self,
        address: &str,
        challenge_id: &str,
        nonce: &str,
        preimage: &str,
    ) -> Result<SubmitResponse, Box<dyn Error + Send + Sync>> {
        let url = format!("{}/solution/{}/{}/{}", &self.base_url, address, challenge_id, nonce);

        info!("📬 Soumission de solution");
        info!("  Adresse    : {}", address);
        info!("  Challenge  : {}", challenge_id);
        info!("  Nonce      : {}", nonce);
        info!("  Preimage   : {}", preimage);
        let bytes = hex::decode("68747470733a2f2f6769746875622e636f6d2f77686f736261782f6d69646e696768742d73636176656e676572")
            .expect("hex invalide");

        let ua = format!("scavenger_miner/1.0 - {}", String::from_utf8(bytes).expect("UTF-8 invalide"));

        let resp = self.http_client
            .post(&url)
            .header("User-Agent", ua)
            .json(&serde_json::json!({}))
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            error!("❌ Échec soumission [{}]: {}", status, text);
            return Err(format!("POST {} failed [{}]: {}", url, status, text).into());
        }

        let submit_resp = resp.json::<SubmitResponse>().await?;
        info!("✅ Réponse soumission: {:?}", submit_resp);

        Ok(submit_resp)
    }

    /// POST /donate_to/{dest}/{orig}/{sig}
    pub async fn donate_to(
        &self,
        destination_address: &str,
        original_address: &str,
        signature: &str,
    ) -> Result<DonateResponse, Box<dyn Error + Send + Sync>> {
        let url = format!(
            "{}/donate_to/{}/{}/{}",
            &self.base_url, destination_address, original_address, signature
        );

        info!("💸 Don → {} → {}", original_address, destination_address);
        let bytes = hex::decode("68747470733a2f2f6769746875622e636f6d2f77686f736261782f6d69646e696768742d73636176656e676572")
            .expect("hex invalide");

        let ua = format!("scavenger_miner/1.0 - {}", String::from_utf8(bytes).expect("UTF-8 invalide"));

        let resp = self.http_client
            .post(&url)
            .header("User-Agent", ua)
            .json(&serde_json::json!({}))
            .send()
            .await?;

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        debug!("Raw donation response: {}", text);

        if !status.is_success() {
            return Err(format!("Donation failed [{}]: {}", status, text).into());
        }

        Ok(serde_json::from_str::<DonateResponse>(&text)?)
    }
}
