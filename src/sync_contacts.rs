// sync_contacts: read a Google Form's response sheet (as CSV), diff against
// whatsapp/contacts.json, and dispatch unknown numbers to a Node helper
// that verifies them via Baileys' onWhatsApp() and optionally sends a
// welcome message. Only registered (= reachable on WhatsApp) numbers are
// written to the store, so the next run retries unregistered ones.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Serialize, Debug)]
pub struct JobContact<'a> {
    pub number: &'a str,
    pub jid: &'a str,
    #[serde(rename = "firstName")]
    pub first_name: &'a str,
    #[serde(rename = "lastName")]
    pub last_name: &'a str,
}

#[derive(Serialize)]
pub struct Job<'a> {
    pub contacts: Vec<JobContact<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome: Option<String>,
    #[serde(rename = "imagePath", skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    #[serde(rename = "groupJid", skip_serializing_if = "Option::is_none")]
    pub group_jid: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct ResultEntry {
    pub number: String,
    pub jid: String,
    pub registered: bool,
    pub sent: bool,
    #[serde(default)]
    pub error: Option<String>,
}

pub fn normalize_phone(raw: &str, default_cc: &str) -> Option<String> {
    // 1) Extract phone-shaped tokens from the cell. A token is '+' or a digit
    //    followed by digits and "in-number" separators (spaces, dashes, dots,
    //    parens). Anything else (letters, multi-byte punctuation, colons)
    //    terminates the current token.
    fn is_sep(b: u8) -> bool {
        matches!(b, b' ' | b'\t' | b'-' | b'(' | b')' | b'.' | b',' | b'/')
    }
    let bytes = raw.as_bytes();
    let n = bytes.len();
    let mut candidates: Vec<(usize, String)> = Vec::new(); // (byte_start, "+digits" or "digits")
    let mut i = 0;
    while i < n {
        let b = bytes[i];
        if b == b'+' || b.is_ascii_digit() {
            let start = i;
            let has_plus = b == b'+';
            if has_plus { i += 1; }
            let mut digits = String::new();
            while i < n {
                let c = bytes[i];
                if c.is_ascii_digit() { digits.push(c as char); i += 1; }
                else if is_sep(c)     { i += 1; }
                else                  { break; }
            }
            if digits.len() >= 7 {
                let token = if has_plus { format!("+{}", digits) } else { digits };
                candidates.push((start, token));
            }
        } else {
            i += 1;
        }
    }
    if candidates.is_empty() { return None; }

    // 2) Multi-'+' rule: if several '+'-prefixed candidates coexist AND the
    //    word "whatsapp" appears in the cell, prefer the candidate nearest
    //    to it (handles "+41 77 ... (Whatsapp: +48...)"). Otherwise pick the
    //    first '+' candidate, or — failing that — the first candidate at all.
    let plus_count = candidates.iter().filter(|c| c.1.starts_with('+')).count();
    let chosen = if plus_count > 1 {
        let lower = raw.to_ascii_lowercase();
        let wa_pos = lower.find("whatsapp");
        candidates.iter()
            .filter(|c| c.1.starts_with('+'))
            .min_by_key(|c| match wa_pos {
                Some(p) => c.0.abs_diff(p),
                None => c.0, // no annotation → first '+' wins
            })
            .map(|c| c.1.clone())
            .unwrap()
    } else {
        candidates[0].1.clone()
    };

    // 3) Country-code normalisation. Order matters: "00" must be tested
    //    before "0".
    let normalized = if chosen.starts_with('+') {
        chosen
    } else if let Some(rest) = chosen.strip_prefix("00") {
        format!("+{}", rest)
    } else if let Some(rest) = chosen.strip_prefix('0') {
        format!("+{}{}", default_cc, rest)
    } else if default_cc == "41" && chosen.len() == 9 && chosen.starts_with('7') {
        // Swiss heuristic: a bare 9-digit mobile (e.g. "779146476") is the
        // local format minus the leading zero. Common form on this list.
        format!("+41{}", chosen)
    } else {
        format!("+{}", chosen)
    };

    // 4) Length sanity. E.164 allows up to 15 digits; under 10 is a typo.
    //    Swiss subscriber numbers are exactly 9 digits → +41 followed by 9.
    let digits = normalized.chars().filter(|c| c.is_ascii_digit()).count();
    if !(10..=15).contains(&digits) { return None; }
    if normalized.starts_with("+41") && digits != 11 { return None; }

    Some(normalized)
}

pub fn jid_for(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("{}@s.whatsapp.net", digits)
}

pub fn db_path(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("whatsapp").join(file_name)
}

pub fn open_db(file_name: &str) -> Result<Connection, Box<dyn std::error::Error>> {
    let conn = Connection::open(db_path(file_name))?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS submissions (
            row_index   INTEGER PRIMARY KEY,
            fetched_at  TEXT NOT NULL,
            data        TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS contacts (
            jid           TEXT PRIMARY KEY,
            number        TEXT NOT NULL,
            first_name    TEXT,
            last_name     TEXT,
            row_index     INTEGER,
            added_at      TEXT NOT NULL,
            FOREIGN KEY (row_index) REFERENCES submissions(row_index)
        );",
    )?;
    Ok(conn)
}

pub fn load_known_jids(conn: &Connection) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare("SELECT jid FROM contacts")?;
    let jids = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<HashSet<_>, _>>()?;
    Ok(jids)
}

pub fn count_contacts(conn: &Connection) -> Result<i64, Box<dyn std::error::Error>> {
    Ok(conn.query_row("SELECT COUNT(*) FROM contacts", [], |r| r.get(0))?)
}

pub fn count_submissions(conn: &Connection) -> Result<i64, Box<dyn std::error::Error>> {
    Ok(conn.query_row("SELECT COUNT(*) FROM submissions", [], |r| r.get(0))?)
}

#[derive(Default, Debug)]
pub struct SyncStats {
    pub inserted: usize,
    pub updated: usize,
    pub new_columns: Vec<String>,
    pub total: usize,
}

/// Mirror the sheet into `submissions`. The `data` column always holds the
/// full row as a JSON object keyed by header name (source of truth for any
/// schema evolution). In addition, every sheet column gets its own TEXT
/// column on the table — added on the fly via `ALTER TABLE` when a new
/// header appears — so values are directly queryable without `json_extract`.
/// `rows[0]` is the header row; data starts at index 1.
pub fn store_submissions(
    conn: &mut Connection,
    rows: &[Vec<String>],
) -> Result<SyncStats, Box<dyn std::error::Error>> {
    if rows.is_empty() { return Ok(SyncStats::default()); }
    let headers = &rows[0];
    let now = chrono::Utc::now().to_rfc3339();

    // Build header → sanitized-column-name mapping with dedup on collisions.
    let mut mapping: Vec<(String, String)> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    for (i, h) in headers.iter().enumerate() {
        let mut col = sanitize_col(h);
        if col.is_empty() { col = format!("col_{}", i + 1); }
        if used.contains(&col) {
            let mut n = 2;
            while used.contains(&format!("{}_{}", col, n)) { n += 1; }
            col = format!("{}_{}", col, n);
        }
        used.insert(col.clone());
        mapping.push((h.clone(), col));
    }

    // Add any newly-seen columns to the table, then backfill them from the
    // existing JSON so old rows aren't left with NULLs.
    let existing_cols: HashSet<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(submissions)")?;
        stmt.query_map([], |r| r.get::<_, String>(1))?
            .collect::<Result<_, _>>()?
    };
    let mut new_cols: Vec<String> = Vec::new();
    for (header, col) in &mapping {
        if !existing_cols.contains(col) {
            // Sanitized col is ASCII [a-z0-9_], safe to interpolate.
            conn.execute(&format!("ALTER TABLE submissions ADD COLUMN \"{}\" TEXT", col), [])?;
            // Header may contain quotes; escape for JSON path.
            let path = format!("$.\"{}\"", header.replace('"', "\\\""));
            conn.execute(
                &format!("UPDATE submissions SET \"{}\" = json_extract(data, ?1)", col),
                params![path],
            )?;
            new_cols.push(col.clone());
        }
    }

    // Snapshot existing row_index set so we can classify insert vs update.
    let existing_indices: HashSet<i64> = {
        let mut stmt = conn.prepare("SELECT row_index FROM submissions")?;
        stmt.query_map([], |r| r.get::<_, i64>(0))?
            .collect::<Result<_, _>>()?
    };

    let col_list: String = mapping.iter()
        .map(|(_, c)| format!("\"{}\"", c)).collect::<Vec<_>>().join(", ");
    let q_marks: String = (4..4 + mapping.len())
        .map(|i| format!("?{}", i)).collect::<Vec<_>>().join(", ");
    let update_assignments: String = mapping.iter()
        .map(|(_, c)| format!("\"{}\" = excluded.\"{}\"", c, c))
        .collect::<Vec<_>>().join(", ");
    let sql = format!(
        "INSERT INTO submissions (row_index, fetched_at, data, {})
         VALUES (?1, ?2, ?3, {})
         ON CONFLICT(row_index) DO UPDATE SET
            fetched_at = excluded.fetched_at,
            data       = excluded.data,
            {}",
        col_list, q_marks, update_assignments
    );

    let tx = conn.transaction()?;
    let mut inserted = 0usize;
    let mut updated = 0usize;
    {
        let mut stmt = tx.prepare(&sql)?;
        for (i, row) in rows.iter().enumerate().skip(1) {
            let row_index = (i + 1) as i64; // 1-based, header is row 1
            let mut obj = serde_json::Map::new();
            for (col_idx, header) in headers.iter().enumerate() {
                let val = row.get(col_idx).cloned().unwrap_or_default();
                obj.insert(header.clone(), serde_json::Value::String(val));
            }
            let json = serde_json::to_string(&obj)?;

            let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![
                Box::new(row_index),
                Box::new(now.clone()),
                Box::new(json),
            ];
            for col_idx in 0..headers.len() {
                let val = row.get(col_idx).cloned().unwrap_or_default();
                params.push(Box::new(val));
            }
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            stmt.execute(rusqlite::params_from_iter(refs))?;
            if existing_indices.contains(&row_index) { updated += 1; } else { inserted += 1; }
        }
    }
    tx.commit()?;

    let total: i64 = conn.query_row("SELECT COUNT(*) FROM submissions", [], |r| r.get(0))?;
    Ok(SyncStats { inserted, updated, new_columns: new_cols, total: total as usize })
}

/// Sanitize a sheet header into a lowercase SQL-safe identifier. Non-alnum
/// chars become `_`; runs collapse; leading/trailing `_` trimmed; capped at
/// 50 chars to avoid absurd column names.
fn sanitize_col(header: &str) -> String {
    let mut out = String::new();
    let mut last_under = true;
    for c in header.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_under = false;
        } else if !last_under {
            out.push('_');
            last_under = true;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.len() > 50 { trimmed.chars().take(50).collect() } else { trimmed }
}

pub fn insert_contact(
    conn: &Connection,
    jid: &str,
    number: &str,
    first_name: &str,
    last_name: &str,
    row_index: Option<i64>,
    added_at: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // INSERT OR IGNORE — reruns are idempotent if the JID is already known.
    conn.execute(
        "INSERT OR IGNORE INTO contacts (jid, number, first_name, last_name, row_index, added_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![jid, number, first_name, last_name, row_index, added_at],
    )?;
    Ok(())
}

pub fn parse_sheet_id_and_gid(input: &str) -> (String, String) {
    let id = if let Some(start) = input.find("/spreadsheets/d/") {
        let after = &input[start + "/spreadsheets/d/".len()..];
        let end = after.find('/').unwrap_or(after.len());
        after[..end].to_string()
    } else {
        input.to_string()
    };
    let gid = input.find("gid=")
        .map(|i| {
            let rest = &input[i + 4..];
            let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            rest[..end].to_string()
        })
        .unwrap_or_else(|| "0".to_string());
    (id, gid)
}

pub fn col_to_idx(s: &str) -> Option<usize> {
    let s = s.trim();
    if let Ok(n) = s.parse::<usize>() {
        return Some(n);
    }
    let up = s.to_ascii_uppercase();
    if up.is_empty() { return None; }
    let mut idx: usize = 0;
    for c in up.chars() {
        if !c.is_ascii_uppercase() { return None; }
        idx = idx * 26 + (c as usize - 'A' as usize + 1);
    }
    Some(idx - 1)
}

#[cfg(test)]
mod tests {
    // Test inputs are synthetic / placeholder numbers (000-padded or US-555
    // fiction range) — never real subscribers. They exercise the same parser
    // paths as production data without carrying personal data into git.
    use super::*;

    fn p(s: &str) -> Option<String> { normalize_phone(s, "41") }

    #[test]
    fn swiss_with_leading_zero_and_spaces() {
        assert_eq!(p("079 000 00 01"), Some("+41790000001".into()));
        assert_eq!(p("0760000002"),    Some("+41760000002".into()));
        assert_eq!(p("076 000 00 03"), Some("+41760000003".into()));
    }

    #[test]
    fn already_international() {
        assert_eq!(p("+41760000004"),  Some("+41760000004".into()));
        assert_eq!(p("+43000000001"),  Some("+43000000001".into()));
        assert_eq!(p("+33600000001"),  Some("+33600000001".into()));
        assert_eq!(p("+85200000001"),  Some("+85200000001".into()));
        assert_eq!(p("+34 600000001"), Some("+34600000001".into()));
    }

    #[test]
    fn double_zero_prefix() {
        assert_eq!(p("0041760000005  "), Some("+41760000005".into()));
        assert_eq!(p("0049000000001"),   Some("+49000000001".into()));
    }

    #[test]
    fn bare_9_digit_swiss() {
        // Bare 9-digit Swiss mobile (missing leading 0) → +41-prefixed via heuristic.
        assert_eq!(p("790000006"), Some("+41790000006".into()));
    }

    #[test]
    fn annotated_single_number() {
        // "+1 555 555-0100" is in the reserved-for-fiction US range.
        assert_eq!(p("+1 555 555 0100 whatsapp"), Some("+15555550100".into()));
        // Full-width Chinese parens around the annotation must not break parsing.
        assert_eq!(p("+8600000000001 （whatsapp）"), Some("+8600000000001".into()));
    }

    #[test]
    fn two_numbers_picks_whatsapp_one() {
        // Two +-prefixed candidates in one cell; "whatsapp" disambiguates.
        assert_eq!(
            p("+41 76 000 00 07 (Whatsapp: +48000000001)"),
            Some("+48000000001".into())
        );
    }

    #[test]
    fn rejects_typos() {
        // 1 digit short of a Swiss mobile (would normalize to 10 digits total).
        assert_eq!(p("079000006"), None);
        // Foreign number with non-+41 country code and 10 digits: passes the
        // basic range check. Will fail later at the WhatsApp lookup step.
        assert_eq!(p("+4230000001"), Some("+4230000001".into()));
    }

    #[test]
    fn empty_or_garbage() {
        assert_eq!(p(""), None);
        assert_eq!(p("abc"), None);
        assert_eq!(p("123"), None); // too short
    }
}
