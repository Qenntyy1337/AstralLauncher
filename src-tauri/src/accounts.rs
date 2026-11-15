use serde::{Deserialize, Serialize};

use crate::config::{Account, Config};
use crate::http::client;

pub fn offline_uuid(username: &str) -> String {
    let digest = md5::compute(format!("OfflinePlayer:{}", username));
    let mut bytes = digest.0;
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5],
        bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11],
        bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

pub fn add_offline(cfg: &mut Config, username: &str) -> Result<Account, String> {
    let username = username.trim();
    if username.is_empty() || username.len() > 16 {
        return Err("ник должен быть от 1 до 16 символов".to_string());
    }
    if !username.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("ник: только латиница, цифры и подчёркивание".to_string());
    }
    if cfg
        .accounts
        .iter()
        .any(|a| a.username.eq_ignore_ascii_case(username))
    {
        return Err("такой аккаунт уже есть".to_string());
    }
    let account = Account {
        id: crate::util::unique_id("acc"),
        kind: "offline".to_string(),
        username: username.to_string(),
        uuid: offline_uuid(username),
        access_token: String::new(),
        refresh_token: String::new(),
        expires_at_ms: 0,
    };
    cfg.accounts.push(account.clone());
    if cfg.active_account_id.is_none() {
        cfg.active_account_id = Some(account.id.clone());
    }
    Ok(account)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    #[serde(rename = "verification_uri")]
    pub verification_uri: String,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub interval: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct TokenResponse {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub error_description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct XblAuthResponse {
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub display_claims: serde_json::Value,
}

pub async fn ms_login_start(client_id: &str) -> Result<DeviceCodeResponse, String> {
    if client_id.trim().is_empty() {
        return Err(
            "не задан MS Azure client_id (Настройки -> Аккаунты -> Microsoft). Создайте приложение в Azure Portal".to_string(),
        );
    }
    let params = [
        ("client_id", client_id.trim()),
        (
            "scope",
            "XboxLive.signin offline_access",
        ),
    ];
    let resp = client()
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode")
        .form(&params)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| format!("HTTP {} у devicecode", status))?;
    if let Some(err) = value.get("error").and_then(|e| e.as_str()) {
        return Err(format!("devicecode: {} ({})", err,
            value.get("error_description").and_then(|d| d.as_str()).unwrap_or("")));
    }
    serde_json::from_value(value).map_err(|e| e.to_string())
}

async fn request_token(client_id: &str, params: &[(&str, &str)]) -> Result<TokenResponse, String> {
    let mut all: Vec<(&str, &str)> = vec![("client_id", client_id.trim())];
    all.extend_from_slice(params);
    let resp = client()
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&all)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let text = resp.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("token: {}", e))
}

async fn xbl_auth(msa_token: &str) -> Result<(String, String), String> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={}", msa_token)
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });
    let resp = client()
        .post("https://xbl.auth.xboxlive.com/xbl/v1/authenticate")
        .header("x-xbl-contract-version", "2")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let value: XblAuthResponse = resp.json().await.map_err(|e| e.to_string())?;
    let uhs = value
        .display_claims
        .get("xui")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .and_then(|f| f.get("uhs"))
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();
    Ok((value.token, uhs))
}

async fn xsts_authorize(xbl_token: &str) -> Result<(String, String), String> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "http://xboxlive.com",
        "TokenType": "JWT"
    });
    let resp = client()
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .header("x-xbl-contract-version", "2")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let value: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if let Some(xerr) = value.get("XErr").and_then(|x| x.as_i64()) {
        let msg = match xerr {
            2148916233 => "у аккаунта нет Xbox Live",
            2148916238 => "аккаунт младше 18 лет, добавьте его в семью",
            2148916236 => "аккаунт из региона без Xbox Live",
            _ => "ошибка авторизации Xbox (XErr)",
        };
        return Err(format!("{} {}", msg, xerr));
    }
    if !status.is_success() {
        return Err(format!("XSTS HTTP {}", status));
    }
    let parsed: XblAuthResponse = serde_json::from_value(value).map_err(|e| e.to_string())?;
    let uhs = parsed
        .display_claims
        .get("xui")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())
        .and_then(|f| f.get("uhs"))
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();
    Ok((parsed.token, uhs))
}

async fn minecraft_login(xsts_token: &str, uhs: &str) -> Result<(String, u64), String> {
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={};{}", uhs, xsts_token)
    });
    let resp = client()
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let value: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = value
        .get("access_token")
        .and_then(|t| t.as_str())
        .ok_or("Minecraft login: нет access_token")?
        .to_string();
    let expires_in = value
        .get("expires_in")
        .and_then(|e| e.as_u64())
        .unwrap_or(86400);
    Ok((token, expires_in))
}

async fn minecraft_profile(token: &str) -> Result<(String, String), String> {
    let resp = client()
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        if status.as_u16() == 404 {
            return Err("у аккаунта нет профиля Minecraft (игра не куплена)".to_string());
        }
        return Err(format!("profile HTTP {}", status));
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("profile: {}", e))?;
    let id = value
        .get("id")
        .and_then(|i| i.as_str())
        .ok_or("у аккаунта нет профиля Minecraft (игра не куплена)")?
        .to_string();
    let name = value
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or("у аккаунта нет имени профиля")?
        .to_string();
    Ok((id, name))
}

pub async fn ms_login_finish(client_id: &str, device_code: &str) -> Result<Account, String> {
    let token = request_token(
        client_id,
        &[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
        ],
    )
    .await?;
    if let Some(err) = &token.error {
        return Err(format!("{}", err));
    }
    let (xbl, uhs) = xbl_auth(&token.access_token).await?;
    let (xsts, uhs2) = xsts_authorize(&xbl).await?;
    let _ = uhs;
    let (mc_token, expires_in) = minecraft_login(&xsts, &uhs2).await?;
    let (profile_id, name) = minecraft_profile(&mc_token).await?;
    Ok(Account {
        id: crate::util::unique_id("acc"),
        kind: "microsoft".to_string(),
        username: name,
        uuid: profile_id,
        access_token: mc_token,
        refresh_token: token.refresh_token,
        expires_at_ms: crate::util::now_millis() + expires_in.saturating_mul(1000),
    })
}

pub async fn refresh_token(cfg: &Config, account: &Account) -> Result<Account, String> {
    if account.kind != "microsoft" {
        return Ok(account.clone());
    }
    if account.expires_at_ms > crate::util::now_millis() + 120_000 {
        return Ok(account.clone());
    }
    if account.refresh_token.is_empty() {
        return Err("у аккаунта нет refresh_token, войдите заново".to_string());
    }
    if cfg.ms_client_id.trim().is_empty() {
        return Err("не задан MS Azure client_id".to_string());
    }
    let token = request_token(
        &cfg.ms_client_id,
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", &account.refresh_token),
            ("scope", "XboxLive.signin offline_access"),
        ],
    )
    .await?;
    if token.access_token.is_empty() {
        return Err(format!(
            "refresh: {}",
            token.error.unwrap_or_else(|| "unknown".to_string())
        ));
    }
    let mut updated = account.clone();
    updated.access_token = token.access_token;
    updated.refresh_token = token.refresh_token;
    updated.expires_at_ms =
        crate::util::now_millis() + token.expires_in.saturating_mul(1000);
    Ok(updated)
}
