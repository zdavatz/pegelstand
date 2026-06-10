// Microsoft Graph client for OneDrive Personal.
//
// Auth: device-code flow (CLI-friendly, no redirect URI needed). On first run,
// the user is shown a code + URL to authenticate in a browser. Tokens are
// cached locally; the refresh_token is used for subsequent runs.
//
// Used by the `welcome pp` flow to (a) download a Word template by item ID
// and (b) upload customized copies into a target folder.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

// Public client (no secret) registered as "pegelstand" in Azure.
// Multi-tenant + personal Microsoft accounts; public client flows enabled.
const CLIENT_ID: &str = "94638180-2849-4bc2-bc4f-ad55ab606c7a";

// Scopes: Files.ReadWrite for OneDrive content, offline_access for refresh
// tokens, User.Read just because it ships in delegated perms by default.
const SCOPES: &str = "Files.ReadWrite offline_access User.Read";

// "consumers" tenant — required for OneDrive Personal (live.com) accounts.
const AUTH_BASE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const GRAPH_BASE: &str = "https://graph.microsoft.com/v1.0";

pub fn token_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("whatsapp")
        .join("onedrive-token.json")
}

#[derive(Debug, Serialize, Deserialize)]
struct TokenCache {
    access_token: String,
    refresh_token: String,
    /// Unix timestamp (seconds) at which `access_token` expires.
    expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: i64,
    interval: i64,
    #[allow(dead_code)]
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
}

#[derive(Debug, Deserialize)]
struct TokenError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

/// Get a valid access token. If none cached, runs the device-code flow
/// interactively. If cached but expired (or near-expired), refreshes.
pub async fn get_access_token(client: &reqwest::Client) -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(raw) = std::fs::read_to_string(token_path()) {
        if let Ok(mut cache) = serde_json::from_str::<TokenCache>(&raw) {
            let now = chrono::Utc::now().timestamp();
            // 60-second safety margin.
            if cache.expires_at > now + 60 {
                return Ok(cache.access_token);
            }
            // Try refresh.
            match refresh_token(client, &cache.refresh_token).await {
                Ok(t) => {
                    cache.access_token = t.access_token.clone();
                    if let Some(rt) = t.refresh_token { cache.refresh_token = rt; }
                    cache.expires_at = now + t.expires_in;
                    save_cache(&cache)?;
                    return Ok(cache.access_token);
                }
                Err(e) => {
                    eprintln!("  OneDrive token refresh failed ({}), re-authenticating...", e);
                }
            }
        }
    }

    // Interactive device-code login.
    let dc = start_device_code(client).await?;
    println!();
    println!("  ╔═══ OneDrive authentication required ═══");
    println!("  ║");
    println!("  ║  1. Open in browser: {}", dc.verification_uri);
    println!("  ║  2. Enter code:      {}", dc.user_code);
    println!("  ║  3. Sign in with your Microsoft account (zdavatz@...).");
    println!("  ║");
    println!("  ║  Waiting for authentication (expires in {}s)...", dc.expires_in);
    println!("  ╚════════════════════════════════════════");

    let tok = poll_for_token(client, &dc).await?;
    let now = chrono::Utc::now().timestamp();
    let cache = TokenCache {
        access_token: tok.access_token.clone(),
        refresh_token: tok.refresh_token.ok_or("no refresh token in response")?,
        expires_at: now + tok.expires_in,
    };
    save_cache(&cache)?;
    println!("  ✓ OneDrive linked. Token saved to whatsapp/onedrive-token.json");
    Ok(cache.access_token)
}

fn save_cache(c: &TokenCache) -> Result<(), Box<dyn std::error::Error>> {
    let p = token_path();
    std::fs::write(&p, serde_json::to_string_pretty(c)?)?;
    // Restrict permissions on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

async fn start_device_code(client: &reqwest::Client) -> Result<DeviceCodeResponse, Box<dyn std::error::Error>> {
    let url = format!("{}/devicecode", AUTH_BASE);
    let resp = client
        .post(&url)
        .form(&[("client_id", CLIENT_ID), ("scope", SCOPES)])
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(format!("device-code request failed ({}): {}", status, body).into());
    }
    Ok(serde_json::from_str(&body)?)
}

async fn poll_for_token(
    client: &reqwest::Client,
    dc: &DeviceCodeResponse,
) -> Result<TokenResponse, Box<dyn std::error::Error>> {
    let deadline = chrono::Utc::now().timestamp() + dc.expires_in;
    let mut interval = dc.interval.max(1);
    loop {
        tokio::time::sleep(Duration::from_secs(interval as u64)).await;
        if chrono::Utc::now().timestamp() > deadline {
            return Err("device code expired before user completed login".into());
        }
        let resp = client
            .post(format!("{}/token", AUTH_BASE))
            .form(&[
                ("client_id", CLIENT_ID),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &dc.device_code),
            ])
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if status.is_success() {
            return Ok(serde_json::from_str(&body)?);
        }
        let err: TokenError = serde_json::from_str(&body)
            .unwrap_or(TokenError { error: "unknown".into(), error_description: Some(body.clone()) });
        match err.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => { interval += 5; continue; }
            "expired_token" | "code_expired" => return Err("device code expired".into()),
            "authorization_declined" | "access_denied" => return Err("user declined authentication".into()),
            other => return Err(format!("token poll failed: {} — {}", other, err.error_description.unwrap_or_default()).into()),
        }
    }
}

async fn refresh_token(
    client: &reqwest::Client,
    refresh: &str,
) -> Result<TokenResponse, Box<dyn std::error::Error>> {
    let resp = client
        .post(format!("{}/token", AUTH_BASE))
        .form(&[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh),
            ("scope", SCOPES),
        ])
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(format!("refresh failed ({}): {}", status, body).into());
    }
    Ok(serde_json::from_str(&body)?)
}

/// Download a file's binary content by its OneDrive item ID.
pub async fn download_item_by_id(
    client: &reqwest::Client,
    token: &str,
    item_id: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let url = format!("{}/me/drive/items/{}/content", GRAPH_BASE, item_id);
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.bytes().await?.to_vec())
}

/// Upload `content` as `filename` into the folder at `folder_path` (relative
/// to the OneDrive root, e.g. `/Documents/Dokumente/wakethief`). Returns the
/// new item's web URL.
///
/// Uses the simple PUT-content endpoint (good for files < 4 MB; Word templates
/// are well under that).
pub async fn upload_to_folder(
    client: &reqwest::Client,
    token: &str,
    folder_path: &str,
    filename: &str,
    content: &[u8],
) -> Result<String, Box<dyn std::error::Error>> {
    let folder_clean = folder_path.trim_matches('/');
    // URL-encode each path segment so spaces, ä, etc. survive.
    let encoded_folder: String = folder_clean
        .split('/')
        .map(|s| urlencoding::encode(s).into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let encoded_name = urlencoding::encode(filename);
    let url = format!(
        "{}/me/drive/root:/{}/{}:/content",
        GRAPH_BASE, encoded_folder, encoded_name
    );
    let resp = client
        .put(&url)
        .bearer_auth(token)
        .header("Content-Type", "application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        .body(content.to_vec())
        .send()
        .await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(format!("upload failed ({}): {}", status, body).into());
    }
    let v: serde_json::Value = serde_json::from_str(&body)?;
    Ok(v.get("webUrl").and_then(|s| s.as_str()).unwrap_or("").to_string())
}
