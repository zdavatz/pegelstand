// Gmail API sender using Application Default Credentials.
//
// Auth: `gcloud auth application-default login --scopes=...,gmail.send` writes
// an "authorized_user" ADC file (~/.config/gcloud/application_default_credentials.json)
// containing client_id / client_secret / refresh_token. We refresh that against
// Google's token endpoint for a short-lived access token, then POST a MIME
// message to users.messages.send.
//
// Used by the `welcome`/`sync-contacts` flow as a fallback: signups that are
// not reachable on WhatsApp get the welcome (with the Zürichsee PNG attached)
// by e-mail from zdavatz@gmail.com instead.

use base64::Engine;
use serde::Deserialize;
use std::path::PathBuf;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const SEND_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";

/// Locate the ADC file: $GOOGLE_APPLICATION_CREDENTIALS if set, otherwise the
/// well-known gcloud path under $HOME.
pub fn adc_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let pb = PathBuf::from(home).join(".config/gcloud/application_default_credentials.json");
    if pb.exists() {
        Some(pb)
    } else {
        None
    }
}

#[derive(Debug, Deserialize)]
struct Adc {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    #[serde(default)]
    quota_project_id: Option<String>,
    #[serde(rename = "type", default)]
    cred_type: String,
}

#[derive(Debug, Deserialize)]
struct TokenResp {
    access_token: String,
}

pub struct GmailSender {
    access_token: String,
    quota_project: Option<String>,
}

impl GmailSender {
    /// Load ADC and exchange the refresh token for an access token. Returns an
    /// error if the ADC file is missing, malformed, or the refresh is rejected
    /// (e.g. the credentials lack the gmail.send scope).
    pub async fn from_adc(
        client: &reqwest::Client,
    ) -> Result<GmailSender, Box<dyn std::error::Error>> {
        let path = adc_path().ok_or(
            "ADC nicht gefunden — bitte `gcloud auth application-default login \
             --scopes=...,gmail.send` ausführen",
        )?;
        let raw = std::fs::read_to_string(&path)?;
        let adc: Adc = serde_json::from_str(&raw)
            .map_err(|e| format!("ADC-Datei nicht lesbar ({}): {}", path.display(), e))?;
        if adc.cred_type != "authorized_user" {
            return Err(format!(
                "ADC-Typ '{}' nicht unterstützt (authorized_user erwartet)",
                adc.cred_type
            )
            .into());
        }
        let resp = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", adc.client_id.as_str()),
                ("client_secret", adc.client_secret.as_str()),
                ("refresh_token", adc.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(format!("Token-Refresh fehlgeschlagen ({}): {}", status, body).into());
        }
        let tok: TokenResp = serde_json::from_str(&body)?;
        Ok(GmailSender {
            access_token: tok.access_token,
            quota_project: adc.quota_project_id,
        })
    }

    /// Send one e-mail with an optional file attachment.
    /// `attachment` = (filename, bytes, mime-type).
    pub async fn send(
        &self,
        client: &reqwest::Client,
        from: &str,
        to: &str,
        subject: &str,
        body_text: &str,
        attachment: Option<(&str, &[u8], &str)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let raw_mime = build_mime(from, to, subject, body_text, attachment);
        let raw_b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw_mime.as_bytes());
        let payload = serde_json::json!({ "raw": raw_b64 });
        let mut req = client
            .post(SEND_URL)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .json(&payload);
        if let Some(qp) = &self.quota_project {
            req = req.header("x-goog-user-project", qp.as_str());
        }
        let resp = req.send().await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(format!("Gmail-Versand fehlgeschlagen ({}): {}", status, body).into());
        }
        Ok(())
    }
}

/// RFC 2047 encoded-word for non-ASCII header values (Subject etc.).
fn encode_header(s: &str) -> String {
    if s.is_ascii() {
        s.to_string()
    } else {
        let b64 = base64::engine::general_purpose::STANDARD.encode(s.as_bytes());
        format!("=?UTF-8?B?{}?=", b64)
    }
}

/// Wrap a base64 blob to 76-character lines (RFC 2045).
fn wrap76(s: &str) -> String {
    s.as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn build_mime(
    from: &str,
    to: &str,
    subject: &str,
    body_text: &str,
    attachment: Option<(&str, &[u8], &str)>,
) -> String {
    let boundary = "pegelstand_boundary_7c3f1a9b2e";
    let mut m = String::new();
    m.push_str(&format!("From: {}\r\n", from));
    m.push_str(&format!("To: {}\r\n", to));
    m.push_str(&format!("Subject: {}\r\n", encode_header(subject)));
    m.push_str("MIME-Version: 1.0\r\n");

    // Body is always base64 (text/plain, UTF-8) so umlauts and long lines are safe.
    let body_b64 = base64::engine::general_purpose::STANDARD.encode(body_text.as_bytes());

    match attachment {
        Some((fname, bytes, mime)) => {
            m.push_str(&format!(
                "Content-Type: multipart/mixed; boundary=\"{}\"\r\n\r\n",
                boundary
            ));
            // text part
            m.push_str(&format!("--{}\r\n", boundary));
            m.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
            m.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
            m.push_str(&wrap76(&body_b64));
            m.push_str("\r\n");
            // attachment part
            m.push_str(&format!("--{}\r\n", boundary));
            m.push_str(&format!("Content-Type: {}; name=\"{}\"\r\n", mime, fname));
            m.push_str("Content-Transfer-Encoding: base64\r\n");
            m.push_str(&format!(
                "Content-Disposition: attachment; filename=\"{}\"\r\n\r\n",
                fname
            ));
            let att_b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
            m.push_str(&wrap76(&att_b64));
            m.push_str(&format!("\r\n--{}--\r\n", boundary));
        }
        None => {
            m.push_str("Content-Type: text/plain; charset=\"UTF-8\"\r\n");
            m.push_str("Content-Transfer-Encoding: base64\r\n\r\n");
            m.push_str(&wrap76(&body_b64));
        }
    }
    m
}
