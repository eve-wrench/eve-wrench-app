use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;
use tokio::sync::RwLock;

const ESI_BASE: &str = "https://esi.evetech.net/latest";

pub static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent("EVE-Wrench/1.0")
        .build()
        .expect("Failed to create HTTP client")
});

static CHAR_CACHE: LazyLock<RwLock<HashMap<i64, CharacterInfo>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

static CORP_CACHE: LazyLock<RwLock<HashMap<i32, CorporationInfo>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterInfo {
    pub character_id: i64,
    pub name: String,
    pub corporation_id: i32,
    pub corporation_name: Option<String>,
    pub alliance_id: Option<i32>,
    pub birthday: String,
    pub security_status: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EsiCharacterResponse {
    name: String,
    corporation_id: i32,
    alliance_id: Option<i32>,
    birthday: String,
    security_status: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorporationInfo {
    pub corporation_id: i32,
    pub name: String,
    pub ticker: String,
    pub member_count: i32,
    pub alliance_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EsiCorporationResponse {
    name: String,
    ticker: String,
    member_count: i32,
    alliance_id: Option<i32>,
}

async fn fetch_character_from_esi(character_id: i64) -> Result<CharacterInfo, String> {
    let url = format!("{}/characters/{}/", ESI_BASE, character_id);

    let response = CLIENT
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("ESI returned status {}", response.status()));
    }

    let esi_char: EsiCharacterResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(CharacterInfo {
        character_id,
        name: esi_char.name,
        corporation_id: esi_char.corporation_id,
        corporation_name: None,
        alliance_id: esi_char.alliance_id,
        birthday: esi_char.birthday,
        security_status: esi_char.security_status,
    })
}

async fn fetch_corporation_from_esi(corporation_id: i32) -> Result<CorporationInfo, String> {
    let url = format!("{}/corporations/{}/", ESI_BASE, corporation_id);

    let response = CLIENT
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("ESI returned status {}", response.status()));
    }

    let esi_corp: EsiCorporationResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    Ok(CorporationInfo {
        corporation_id,
        name: esi_corp.name,
        ticker: esi_corp.ticker,
        member_count: esi_corp.member_count,
        alliance_id: esi_corp.alliance_id,
    })
}

pub async fn get_character(character_id: i64) -> Result<CharacterInfo, String> {
    {
        let cache = CHAR_CACHE.read().await;
        if let Some(info) = cache.get(&character_id) {
            return Ok(info.clone());
        }
    }

    let mut info = fetch_character_from_esi(character_id).await?;

    if let Ok(corp) = get_corporation(info.corporation_id).await {
        info.corporation_name = Some(corp.name);
    }

    {
        let mut cache = CHAR_CACHE.write().await;
        cache.insert(character_id, info.clone());
    }

    Ok(info)
}

pub async fn get_corporation(corporation_id: i32) -> Result<CorporationInfo, String> {
    {
        let cache = CORP_CACHE.read().await;
        if let Some(info) = cache.get(&corporation_id) {
            return Ok(info.clone());
        }
    }

    let info = fetch_corporation_from_esi(corporation_id).await?;

    {
        let mut cache = CORP_CACHE.write().await;
        cache.insert(corporation_id, info.clone());
    }

    Ok(info)
}
