// Minimal Google Sheets read-only client using service-account JWT auth.
// One JSON key + a shared sheet → no browser flow, no token cache, no
// callback server. Token is short-lived (1 h) and minted per command run.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(Serialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResp { access_token: String }

#[derive(Deserialize)]
struct ValuesResp {
    #[serde(default)]
    values: Vec<Vec<String>>,
}

#[derive(Deserialize)]
struct SheetProps {
    #[serde(rename = "sheetId")]
    sheet_id: u64,
    title: String,
}

#[derive(Deserialize)]
struct SheetEntry { properties: SheetProps }

#[derive(Deserialize)]
struct SpreadsheetMeta { sheets: Vec<SheetEntry> }

pub fn key_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("whatsapp").join("google-sa.json")
}

/// Service-account email pulled from the key file, for error messages.
pub fn key_client_email(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let key: ServiceAccountKey = serde_json::from_str(&raw).ok()?;
    Some(key.client_email)
}

pub async fn fetch_access_token(
    client: &reqwest::Client,
    key_path: &Path,
    scope: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(key_path)?;
    let key: ServiceAccountKey = serde_json::from_str(&raw)?;

    let now = chrono::Utc::now().timestamp() as u64;
    let claims = Claims {
        iss: key.client_email.clone(),
        scope: scope.to_string(),
        aud: key.token_uri.clone(),
        iat: now,
        exp: now + 3600,
    };
    let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
    let enc = jsonwebtoken::EncodingKey::from_rsa_pem(key.private_key.as_bytes())?;
    let jwt = jsonwebtoken::encode(&header, &claims, &enc)?;

    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", jwt.as_str()),
    ];
    let resp = client.post(&key.token_uri).form(&params).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(format!("Token-Exchange fehlgeschlagen ({}): {}", status, body).into());
    }
    let parsed: TokenResp = serde_json::from_str(&body)?;
    Ok(parsed.access_token)
}

pub async fn resolve_sheet_title(
    client: &reqwest::Client,
    token: &str,
    spreadsheet_id: &str,
    gid: u64,
) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}?fields=sheets.properties",
        spreadsheet_id
    );
    let resp = client.get(&url).bearer_auth(token).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(format!("Sheets-API metadata fehlgeschlagen ({}): {}", status, body).into());
    }
    let meta: SpreadsheetMeta = serde_json::from_str(&body)?;
    meta.sheets.into_iter()
        .find(|s| s.properties.sheet_id == gid)
        .map(|s| s.properties.title)
        .ok_or_else(|| format!("gid {} nicht im Spreadsheet gefunden", gid).into())
}

pub async fn fetch_values(
    client: &reqwest::Client,
    token: &str,
    spreadsheet_id: &str,
    range: &str,
) -> Result<Vec<Vec<String>>, Box<dyn std::error::Error>> {
    let encoded = url_encode(range);
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        spreadsheet_id, encoded
    );
    let resp = client.get(&url).bearer_auth(token).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        return Err(format!("Sheets-API values.get fehlgeschlagen ({}): {}", status, body).into());
    }
    let parsed: ValuesResp = serde_json::from_str(&body)?;
    Ok(parsed.values)
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}
