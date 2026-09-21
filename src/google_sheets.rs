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

/// Append rows at the bottom of `range` (e.g. `'Vergangene Lektionen'!A:K`).
/// Needs the write scope `https://www.googleapis.com/auth/spreadsheets` and
/// the sheet shared with the SA as Editor.
pub async fn append_values(
    client: &reqwest::Client,
    token: &str,
    spreadsheet_id: &str,
    range: &str,
    values: &[Vec<String>],
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}:append?valueInputOption=RAW&insertDataOption=INSERT_ROWS",
        spreadsheet_id, url_encode(range)
    );
    let body = serde_json::json!({ "values": values });
    let resp = client.post(&url).bearer_auth(token).json(&body).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        return Err(format!("Sheets-API values.append fehlgeschlagen ({}): {}", status, t).into());
    }
    Ok(())
}

/// Delete whole rows from the tab `gid`. `row_ranges` are inclusive 1-based
/// sheet row numbers `(first, last)`; they are applied bottom-up in one
/// batchUpdate so the indices stay valid.
pub async fn delete_rows(
    client: &reqwest::Client,
    token: &str,
    spreadsheet_id: &str,
    gid: u64,
    row_ranges: &[(usize, usize)],
) -> Result<(), Box<dyn std::error::Error>> {
    if row_ranges.is_empty() {
        return Ok(());
    }
    let mut ranges: Vec<(usize, usize)> = row_ranges.to_vec();
    ranges.sort_by(|a, b| b.0.cmp(&a.0));
    let requests: Vec<serde_json::Value> = ranges.iter().map(|(a, b)| serde_json::json!({
        "deleteDimension": { "range": {
            "sheetId": gid, "dimension": "ROWS",
            "startIndex": a - 1, "endIndex": b } }
    })).collect();
    let url = format!("https://sheets.googleapis.com/v4/spreadsheets/{}:batchUpdate", spreadsheet_id);
    let resp = client.post(&url).bearer_auth(token)
        .json(&serde_json::json!({ "requests": requests })).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        return Err(format!("Sheets-API batchUpdate (deleteDimension) fehlgeschlagen ({}): {}", status, t).into());
    }
    Ok(())
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
