//! Minimal Google Calendar v3 write client for the `welcome` flow.
//!
//! Auth is the same service account as Sheets (`google_sheets::fetch_access_token`
//! with the `calendar.events` scope). The target calendar must be shared with
//! the SA's e-mail as "Änderungen an Terminen vornehmen"; the Calendar API must
//! be enabled in the `pegelstand` GCP project. Both are one-time steps.
//!
//! Every event we create carries `extendedProperties.private.pegelstand_phone`
//! so re-runs (e.g. a contact deleted from the DB and greeted again) stay
//! idempotent via `event_exists`.

use reqwest::Client;
use serde_json::Value;

const BASE: &str = "https://www.googleapis.com/calendar/v3";

/// True if an event tagged with `phone` already exists on `day` (YYYY-MM-DD).
pub async fn event_exists(
    client: &Client,
    token: &str,
    cal_id: &str,
    day: &str,
    phone: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let url = format!("{}/calendars/{}/events", BASE, urlencode(cal_id));
    let resp = client
        .get(&url)
        .bearer_auth(token)
        .query(&[
            ("privateExtendedProperty", format!("pegelstand_phone={}", phone)),
            ("timeMin", format!("{}T00:00:00Z", day)),
            ("timeMax", format!("{}T23:59:59Z", day)),
            ("singleEvents", "true".to_string()),
        ])
        .send()
        .await?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        return Err(format!("Calendar list fehlgeschlagen ({}): {}", status, t).into());
    }
    let v: Value = resp.json().await?;
    Ok(v.get("items")
        .and_then(|i| i.as_array())
        .map_or(false, |a| !a.is_empty()))
}

/// Insert one event; `body` is the full Calendar event resource.
pub async fn insert_event(
    client: &Client,
    token: &str,
    cal_id: &str,
    body: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("{}/calendars/{}/events", BASE, urlencode(cal_id));
    let resp = client.post(&url).bearer_auth(token).json(body).send().await?;
    let status = resp.status();
    if !status.is_success() {
        let t = resp.text().await.unwrap_or_default();
        return Err(format!("Calendar insert fehlgeschlagen ({}): {}", status, t).into());
    }
    Ok(())
}

/// Calendar IDs are e-mail addresses — escape the reserved characters that
/// may appear in one so they survive inside the URL path.
fn urlencode(s: &str) -> String {
    s.replace('%', "%25")
        .replace('@', "%40")
        .replace('/', "%2F")
        .replace(':', "%3A")
}
