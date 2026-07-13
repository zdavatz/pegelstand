// One-time OAuth helper: mint a refresh token for pegelstand's OWN Gmail OAuth
// client (scope gmail.send) via the loopback flow, and write it to
// whatsapp/google-oauth-token.json for the `welcome` / `--force-email` sender
// (src/gmail.rs). This replaces gcloud ADC, which Google now blocks from the
// gmail.send scope ("cloud-platform required" / "Diese App ist blockiert").
//
// Prerequisite: whatsapp/google-oauth-client.json — the "Desktop app" OAuth
// client downloaded from the Google Cloud Console (an object with an "installed"
// key holding client_id / client_secret). Both files are gitignored.
//
// Usage:
//   cargo run --release --bin gmail_auth
//     → prints an auth URL. Open it in a browser on THIS machine, or forward the
//       loopback port from your laptop first:  ssh -L <port>:localhost:<port> host
//       (the helper prints the exact port). Approve access as zdavatz@gmail.com;
//       it captures the redirect on 127.0.0.1 and saves the refresh token.
//
// Note: while the OAuth app is still "unverified", Google expires the refresh
// token after 7 days. After Google's sensitive-scope verification it is
// indefinite, so `--force-email` then runs fully automatically.

use serde::Deserialize;
use std::io::{Read, Write};
use std::net::TcpListener;

const SCOPE: &str = "https://www.googleapis.com/auth/gmail.send";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

#[derive(Deserialize)]
struct Installed {
    client_id: String,
    client_secret: String,
}
#[derive(Deserialize)]
struct ClientFile {
    installed: Installed,
}

#[derive(Deserialize)]
struct TokenResp {
    refresh_token: Option<String>,
    scope: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let client_path = format!("{}/whatsapp/google-oauth-client.json", manifest);
    let raw = std::fs::read_to_string(&client_path).map_err(|e| {
        format!(
            "{} nicht lesbar: {}\n  → Lade zuerst den OAuth-'Desktop'-Client als JSON aus der \
             Google Cloud Console (APIs & Dienste → Anmeldedaten → Client herunterladen) dorthin.",
            client_path, e
        )
    })?;
    let cf: ClientFile = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "{} ist kein gültiges 'installed'-Client-JSON ({}). \
             Erwartet wird die von Google heruntergeladene Datei mit oberstem Schlüssel \"installed\".",
            client_path, e
        )
    })?;
    let client_id = cf.installed.client_id;
    let client_secret = cf.installed.client_secret;

    // Loopback server on a free port — Desktop clients accept any 127.0.0.1 port
    // without pre-registration.
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let redirect = format!("http://127.0.0.1:{}", port);

    let state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .to_string();
    let auth = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &access_type=offline&prompt=consent&state={}",
        AUTH_URL,
        urlencoding::encode(&client_id),
        urlencoding::encode(&redirect),
        urlencoding::encode(SCOPE),
        state,
    );

    println!("\n1) Öffne diese URL im Browser (auf DIESEM Rechner, oder vom Laptop via");
    println!("   `ssh -L {port}:localhost:{port} <host>` und dann lokal öffnen):\n");
    println!("{auth}\n");
    println!("2) Als zdavatz@gmail.com anmelden. Bei \"nicht bestätigte App\":");
    println!("   \"Erweitert\" → \"Weiter zu … (unsicher)\", dann Zugriff erlauben.");
    println!("   Lausche auf {redirect} …");

    // Accept exactly one connection and parse ?code=... from the request line.
    let (mut stream, _) = listener.accept()?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf)?;
    let req = String::from_utf8_lossy(&buf[..n]);
    let first_line = req.lines().next().unwrap_or("");
    let query = first_line
        .split_whitespace()
        .nth(1)
        .and_then(|path| path.split_once('?').map(|(_, q)| q))
        .unwrap_or("");
    let get_param = |key: &str| -> Option<String> {
        query.split('&').find_map(|kv| {
            let (k, v) = kv.split_once('=')?;
            if k == key {
                Some(
                    urlencoding::decode(v)
                        .map(|s| s.into_owned())
                        .unwrap_or_else(|_| v.to_string()),
                )
            } else {
                None
            }
        })
    };

    // Reply so the browser shows something friendly, then close.
    let (title, msg) = if get_param("error").is_some() {
        ("Authentifizierung abgebrochen", "Es wurde kein Zugriff gewährt. Bitte den Befehl erneut ausführen.")
    } else {
        ("pegelstand: Authentifizierung erhalten ✓", "Du kannst dieses Fenster schließen.")
    };
    let html = format!(
        "<html><body style=\"font-family:sans-serif;max-width:32em;margin:4em auto\">\
         <h3>{title}</h3><p>{msg}</p></body></html>"
    );
    let resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(resp.as_bytes());

    if let Some(err) = get_param("error") {
        return Err(format!("OAuth-Fehler im Redirect: {}", err).into());
    }
    let code = get_param("code").ok_or("Kein 'code' im Redirect gefunden")?;

    // Exchange the authorization code for tokens.
    let client = reqwest::Client::new();
    let r = client
        .post(TOKEN_URL)
        .form(&[
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;
    let status = r.status();
    let body = r.text().await?;
    let tok: TokenResp = serde_json::from_str(&body)
        .map_err(|e| format!("Token-Antwort nicht lesbar ({}): {}", e, body))?;
    if !status.is_success() || tok.error.is_some() {
        return Err(format!(
            "Token-Tausch fehlgeschlagen ({}): {} {}",
            status,
            tok.error.unwrap_or_default(),
            tok.error_description.unwrap_or_default()
        )
        .into());
    }
    let refresh = tok.refresh_token.ok_or(
        "Kein refresh_token erhalten. Widerrufe unter myaccount.google.com/permissions den \
         bestehenden Zugriff dieser App und führe den Befehl erneut aus (access_type=offline & \
         prompt=consent sind bereits gesetzt).",
    )?;

    let out = serde_json::json!({
        "type": "authorized_user",
        "client_id": client_id,
        "client_secret": client_secret,
        "refresh_token": refresh,
    });
    let out_path = format!("{}/whatsapp/google-oauth-token.json", manifest);
    std::fs::write(&out_path, serde_json::to_string_pretty(&out)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&out_path, std::fs::Permissions::from_mode(0o600));
    }

    println!("\n✓ Refresh-Token gespeichert: {out_path}");
    if let Some(s) = tok.scope {
        println!("  Scopes: {s}");
    }
    println!("  → `sync-contacts --force-email` nutzt jetzt diesen OAuth-Client (kein gcloud/ADC mehr).");
    println!("  Hinweis: Solange die App 'nicht verifiziert' ist, läuft das Token nach 7 Tagen ab.");
    println!("  Nach Google's Sensitive-Scope-Verifizierung (gmail.send) bleibt es unbegrenzt gültig.");
    Ok(())
}
