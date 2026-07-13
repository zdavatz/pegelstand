// Gmail API sender.
//
// Auth (two interchangeable sources, both "authorized_user" shape — client_id /
// client_secret / refresh_token — refreshed against Google's token endpoint for
// a short-lived access token, then POST a MIME message to users.messages.send):
//
//   1. Preferred: our OWN dedicated OAuth client. `gmail_auth` (src/bin) runs the
//      loopback flow once and writes whatsapp/google-oauth-token.json. This avoids
//      gcloud entirely — Google now blocks the shared gcloud client from the
//      gmail.send scope ("cloud-platform required" / "Diese App ist blockiert").
//   2. Fallback: gcloud ADC (~/.config/gcloud/application_default_credentials.json)
//      from `gcloud auth application-default login --scopes=...,gmail.send`.
//
// Used by the `welcome`/`sync-contacts` flow: signups not reachable on WhatsApp
// get the welcome (with the Zürichsee PNG attached) by e-mail from zdavatz@gmail.com.

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

/// Locate our dedicated pegelstand OAuth token file (preferred over ADC): a JSON
/// with client_id / client_secret / refresh_token minted by the `gmail_auth`
/// helper against our own OAuth client. Env override: $PEGELSTAND_GMAIL_OAUTH,
/// otherwise whatsapp/google-oauth-token.json next to the crate.
pub fn oauth_token_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("PEGELSTAND_GMAIL_OAUTH") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    let pb = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("whatsapp/google-oauth-token.json");
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
    /// Load OAuth credentials and exchange the refresh token for an access token.
    /// Prefers our dedicated pegelstand OAuth token file (whatsapp/google-oauth-token.json
    /// or $PEGELSTAND_GMAIL_OAUTH); falls back to gcloud ADC. Both carry the same
    /// authorized_user fields and use the same refresh grant. Returns an error if
    /// neither source exists, the file is malformed, or the refresh is rejected
    /// (e.g. the credentials lack the gmail.send scope, or the token expired).
    pub async fn from_credentials(
        client: &reqwest::Client,
    ) -> Result<GmailSender, Box<dyn std::error::Error>> {
        let (path, source) = if let Some(p) = oauth_token_path() {
            (p, "pegelstand-OAuth")
        } else if let Some(p) = adc_path() {
            (p, "gcloud-ADC")
        } else {
            return Err(
                "Keine Gmail-Credentials gefunden — bitte einmalig \
                 `cargo run --release --bin gmail_auth` ausführen (dedizierter OAuth-Client), \
                 alternativ `gcloud auth application-default login --scopes=...,gmail.send`."
                    .into(),
            );
        };
        let raw = std::fs::read_to_string(&path)?;
        let adc: Adc = serde_json::from_str(&raw).map_err(|e| {
            format!("Credentials-Datei nicht lesbar ({}): {}", path.display(), e)
        })?;
        // Our own token file may omit "type"; ADC sets it to "authorized_user".
        if !adc.cred_type.is_empty() && adc.cred_type != "authorized_user" {
            return Err(format!(
                "Credential-Typ '{}' nicht unterstützt (authorized_user erwartet)",
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
            return Err(
                format!("Token-Refresh fehlgeschlagen ({}) [{}]: {}", status, source, body).into(),
            );
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

// ---------------------------------------------------------------------------
// SMTP sender (Gmail App Password) — the simple, no-OAuth alternative.
//
// A 16-char Gmail App Password (requires 2FA on the account) authenticates
// against smtp.gmail.com directly — no OAuth client, no verification, no token
// expiry, no refresh. The password lives OUTSIDE the repo at
// ~/.config/pegelstand/gmail-app-password.txt (or $PEGELSTAND_GMAIL_APP_PASSWORD)
// so it is never committed. Trade-off vs. the gmail.send OAuth scope: an app
// password also permits IMAP read access, so guard the file (chmod 600).
// ---------------------------------------------------------------------------

const SMTP_USER: &str = "zdavatz@gmail.com";

/// Read the Gmail app password: env $PEGELSTAND_GMAIL_APP_PASSWORD first, else
/// ~/.config/pegelstand/gmail-app-password.txt. All whitespace is stripped
/// (Google shows the password grouped as "abcd efgh ijkl mnop"). Returns None
/// if neither source yields a non-empty value.
pub fn app_password() -> Option<String> {
    let clean = |s: String| {
        let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    };
    if let Ok(p) = std::env::var("PEGELSTAND_GMAIL_APP_PASSWORD") {
        if let Some(t) = clean(p) {
            return Some(t);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".config/pegelstand/gmail-app-password.txt");
    clean(std::fs::read_to_string(path).ok()?)
}

pub struct SmtpSender {
    transport: lettre::AsyncSmtpTransport<lettre::Tokio1Executor>,
}

impl SmtpSender {
    /// Build an SMTP transport from the app password, or `None` if no password
    /// is configured (so the caller can fall back to the OAuth API sender).
    /// Connection/auth happen lazily on the first `send`.
    pub fn from_app_password() -> Result<Option<SmtpSender>, Box<dyn std::error::Error>> {
        let pw = match app_password() {
            Some(p) => p,
            None => return Ok(None),
        };
        let creds = lettre::transport::smtp::authentication::Credentials::new(
            SMTP_USER.to_string(),
            pw,
        );
        let transport =
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::relay("smtp.gmail.com")?
                .credentials(creds)
                .build();
        Ok(Some(SmtpSender { transport }))
    }

    /// Send one e-mail with an optional file attachment.
    /// `attachment` = (filename, bytes, mime-type). Same signature as
    /// `GmailSender::send` minus the reqwest client (SMTP needs none).
    pub async fn send(
        &self,
        from: &str,
        to: &str,
        subject: &str,
        body_text: &str,
        attachment: Option<(&str, &[u8], &str)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use lettre::message::{header::ContentType, Attachment, MultiPart, SinglePart};
        use lettre::{AsyncTransport, Message};

        let builder = Message::builder()
            .from(from.parse()?)
            .to(to.parse()?)
            .subject(subject);
        let email = match attachment {
            Some((fname, bytes, mime)) => {
                let att = Attachment::new(fname.to_string())
                    .body(bytes.to_vec(), ContentType::parse(mime)?);
                builder.multipart(
                    MultiPart::mixed()
                        .singlepart(SinglePart::plain(body_text.to_string()))
                        .singlepart(att),
                )?
            }
            None => builder.body(body_text.to_string())?,
        };
        self.transport.send(email).await?;
        Ok(())
    }
}

/// Unified mail sender: prefer SMTP (Gmail App Password) when configured,
/// otherwise the Gmail API (dedicated OAuth client, falling back to gcloud ADC).
/// Lets the `welcome`/`--force-email` flow stay identical regardless of transport.
pub enum Mailer {
    Smtp(SmtpSender),
    Api(GmailSender),
}

impl Mailer {
    pub async fn autodetect(
        client: &reqwest::Client,
    ) -> Result<Mailer, Box<dyn std::error::Error>> {
        if let Some(smtp) = SmtpSender::from_app_password()? {
            return Ok(Mailer::Smtp(smtp));
        }
        Ok(Mailer::Api(GmailSender::from_credentials(client).await?))
    }

    /// Human-readable transport label for logging.
    pub fn transport(&self) -> &'static str {
        match self {
            Mailer::Smtp(_) => "SMTP (App-Passwort)",
            Mailer::Api(_) => "Gmail-API (OAuth)",
        }
    }

    pub async fn send(
        &self,
        client: &reqwest::Client,
        from: &str,
        to: &str,
        subject: &str,
        body_text: &str,
        attachment: Option<(&str, &[u8], &str)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Mailer::Smtp(s) => s.send(from, to, subject, body_text, attachment).await,
            Mailer::Api(a) => a.send(client, from, to, subject, body_text, attachment).await,
        }
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
