# CLAUDE.md

## Project Overview

Rust CLI tool (`pegelstand`) for querying water level, wind, wave, and temperature data for pumpfoiling and wingfoiling. Locations: Zürichsee, Silvaplana, Neuenburgersee, Urnersee, Greifensee, Ermioni (Greece).

## Architecture

Single-binary CLI built with:
- `clap` for argument parsing (derive mode)
- `reqwest` for HTTP requests
- `serde` / `serde_json` for JSON deserialization
- `csv` for InfluxDB CSV response parsing
- `chrono` for date/time handling
- `tokio` for async runtime
- `resvg` for SVG→PNG rendering (pure Rust, no external dependencies)

Code is split across:
- `src/main.rs` — CLI, API clients, all commands
- `src/svg_report.rs` — pure SVG chart generation (no JS dependencies)
- `src/netcdf3.rs` — minimal NetCDF3 Classic reader (pure Rust, no C dependencies)
- `src/google_sheets.rs` — minimal Google Sheets read client (service-account JWT auth via `jsonwebtoken`, no `yup-oauth2`)
- `src/sync_contacts.rs` — phone normalization, SQLite store (`rusqlite` bundled), submissions+contacts tables
- `src/chartjs.min.js` — Chart.js library, embedded at compile time via `include_str!`

## APIs Used

1. **api.existenz.ch** (BAFU hydrology) — base URL: `https://api.existenz.ch/apiv1/hydro`
   - `/locations`, `/parameters`, `/latest` endpoints
   - Note: `LocationDetails.id` can be string or integer — requires custom deserializer (`string_or_int`)
   - Temperature data only available for rivers, not lakes

2. **api.existenz.ch** (SwissMetNet/SMN) — base URL: `https://api.existenz.ch/apiv1/smn`
   - `/latest`, `/daterange`, `/locations`, `/parameters` endpoints
   - Same JSON format as hydro API (timestamp/loc/par/val)
   - Station **SIA** (Segl-Maria) for Silvaplana, **PAY** (Payerne) for Neuenburgersee, **ALT** (Altdorf) for Urnersee, **PFA** (Pfaffikon ZH) for Greifensee
   - Parameters: dd (wind dir), ff (wind speed km/h), fx (gusts km/h), tt (temp), td (dewpoint), rh (humidity), qfe (pressure), rr (precipitation), ss (sunshine min), rad (radiation W/m²)
   - daterange API limited to ~30 days; older data via InfluxDB

3. **InfluxDB** at `https://influx.konzept.space` — historical data via Flux queries
   - Read-only token is public (embedded in code)
   - Bucket: `existenzApi`, org: `api.existenz.ch`
   - Contains both `hydro` and `smn` measurements
   - Lake reports (Silvaplana/Neuenburgersee/Urnersee) auto-fall back to InfluxDB with hourly aggregation for data older than 30 days

3. **Tecdottir** (Stadt Zürich / Wasserschutzpolizei) — `https://tecdottir.metaodi.ch`
   - Zürichsee water temperature and weather at stations `tiefenbrunnen` and `mythenquai`
   - API limit: 1000 records per request, paginate with `offset`
   - `seetemperatur` and `report` commands merge both stations: Tiefenbrunnen provides temperature/wind/pressure, Mythenquai provides precipitation/radiation/water_level
   - All 14 fields: water_temperature, air_temperature, windchill, dew_point, humidity, wind_speed_avg_10min, wind_gust_max_10min, wind_force_avg_10min, wind_direction, barometric_pressure_qfe, precipitation, global_radiation, water_level

4. **Open-Meteo** — `https://api.open-meteo.com/v1/forecast` + `archive-api.open-meteo.com` + `marine-api.open-meteo.com`
   - Used for Ermioni (Greece) — model-based data, no API key needed
   - Forecast (hourly/15-min), archive back to 1940, marine waves
   - Parameters: wind_speed_10m, wind_direction_10m, wind_gusts_10m, temperature_2m, relative_humidity_2m, pressure_msl, wave_height, wind_wave_direction, wind_wave_period

5. **Poseidon/HCMR** — `https://apps.poseidon.hcmr.gr/webapp/poseidon_db/`
   - Greek marine research stations, web login required (zdavatz@gmail.com)
   - Data download via web form → NetCDF (.nc) files per email
   - **Palea Fokea** (37.72°N, 23.95°E, Saronischer Golf, ~50 km NW Ermioni): DRYT (air temp), WSPD (wind speed), WDIR (wind dir), ATMS (pressure), RELH (humidity), SLEV (sea level) — 5-min interval
   - Saronikos buoy: listed in online data table but currently offline (all N/A)
   - API (`api.poseidon.hcmr.gr`): OAuth2 auth, credentials pending
   - Register: https://auth.poseidon.hcmr.gr/auth/register/

## Zürichsee Reglement 1977

The `zurichsee` command evaluates the current water level against the 1977 regulation:
- Regulierlinie varies by month (approximated from the regulation chart)
- Abflussgrenze (lower limit): 405.90 m ü.M.
- Critical high: > 407.50 m ü.M.

## Standalone SVG

The `svg` command generates a pure SVG file (no HTML wrapper) with Zürichsee data:
- Five charts: Temperatur (water + air), Pegelstand, Wind & Böen, Windrichtung (dots 0–360°), Luftdruck
- Uses `write_standalone_svg()` in `svg_report.rs`
- Fetches Tecdottir Tiefenbrunnen (T) + Mythenquai (M), merges by timestamp
- Date format: dd.mm.yyyy throughout
- Default: last 5 days, output to `svg/` directory
- `--png` flag: additionally renders PNG via `resvg` (2x retina), output to `png/` directory
- `--whatsapp <GROUP_JID>` flag: sends PNG to a WhatsApp group via Baileys (requires `--png`)
- PNG export useful for WhatsApp (which doesn't support inline SVG preview)
- X-axis labels: first label uses `text-anchor="start"`, last uses `"end"` to prevent clipping at SVG edges; applies to both standalone SVG and HTML-embedded SVG charts
- Last-value labels: every chart line / dot series renders a coloured dot + numeric label at the latest non-NaN datapoint via `svg_last_value_label()` in `svg_report.rs`. Decimals adapt to the y-unit (2 for `m ü.M.`, 0 for `hPa`, otherwise 1). Anchor flips to `end` if the point is past 85% of plot width, so the label never clips on the right edge. Applied uniformly across `write_standalone_svg`, `write_ermioni_svg`, and `write_paleafokea_svg`.
- `--bg <path>` flag (svg + ermioni): embeds an image as faint background (opacity 0.25, `preserveAspectRatio="xMidYMid slice"`) below the title and charts. Conversion via `prepare_bg_image()` in `main.rs` calls macOS `qlmanage -t -s 1500 -o <tmpdir> <input>` — `qlmanage` honours the HEIC `irot` rotation box, while `sips` ignores it and produces sideways-rotated output for landscape iPhone photos. Output is base64-encoded and inlined as `data:image/png;base64,...` in the SVG. The `base64` crate is the only dep added for this.

## Ermioni SVG/PNG

The `ermioni` command supports `--png`/`--whatsapp` in addition to console output:
- Fetches Open-Meteo hourly (wind/gusts/dir/temp/pressure) + Open-Meteo Marine (wave height)
- Auto-selects forecast vs archive API (archive only if **both** start and end are older than 2 days; mixed past+future ranges use forecast, which covers recent past via `start_date`/`end_date`)
- Five SVG charts: Wind & Böen (escape `&` as `&amp;` in titles!), Windrichtung (dots 0–360°), Lufttemperatur, Wellenhöhe, Luftdruck
- Uses `write_ermioni_svg()` in `svg_report.rs`, modeled on `write_paleafokea_svg`
- Data tuple shape: `(label, wind_speed, gust, wind_dir, temp, wave_height, pressure)` — 7 fields
- Label format parsed from Open-Meteo ISO string `"2026-04-17T00:00"` → `"17.04.2026 00:00"`
- Text console output still works when no `--svg`/`--png`/`--whatsapp` flags are passed

## Palea Fokea (NetCDF3)

The `paleafokea` command reads NetCDF3 Classic files from the Poseidon/HCMR portal:
- Pure Rust NetCDF3 parser in `src/netcdf3.rs` — no libnetcdf C dependency
- Handles record variables (unlimited TIME dimension) with interleaved storage
- Reads variables: TIME (float64, days since 1950-01-01), DRYT (air temp °C), WSPD (wind m/s), WDIR (wind dir °), ATMS (pressure hPa), SLEV (sea level m)
- Fill values (-9999.99) mapped to NaN
- Five SVG charts: Lufttemperatur, Meeresspiegel, Windgeschwindigkeit, Windrichtung (dots, fixed 0–360°), Luftdruck
- Uses `write_paleafokea_svg()` in `svg_report.rs`
- Auto-finds newest `.nc` file in `poseidon_data/`, or specify with `--file`
- Supports `--png` (2x retina via resvg) and `--whatsapp <JID>`
- NetCDF files downloaded manually from https://apps.poseidon.hcmr.gr/webapp/poseidon_db/

## WhatsApp Integration

- Located in `whatsapp/` directory — standalone Node.js scripts using Baileys (`@whiskeysockets/baileys` v7)
- **Requires Node.js ≥ 22** (Baileys v7 segfaults on Node 20.2; nvm path `/home/zeno/.nvm/versions/node/v22.22.2/bin/node`)
- `send.mjs` — send image to a WhatsApp group JID with optional caption; uses `process.exit(0)` after send to avoid close-handler hangs
- `send-doc.mjs` — send any file type (PDF, CSV, log, image, ...) to a JID *or* a plain phone number (`41787496544` is auto-suffixed to `@s.whatsapp.net`). Picks `image:` for png/jpg/jpeg, `document:` for everything else with a mime-type lookup table. Uses a 5-minute connection timeout (room for QR scan) and a 10-second post-send delay so async `saveCreds()` finishes writing `auth/creds.json` before node exits — without the delay, the next send needs a fresh QR.
- `list-groups.mjs` — list all groups with their JIDs
- `login.mjs` — dedicated login flow; auto-wipes stale sessions on 401 (loggedOut) and auto-reconnects on 515 (restartRequired, fires after first QR scan). Uses a `done` flag to prevent open/close race from reporting a spurious error when open fires moments before the socket closes.
- `login-qr.mjs` — same login flow but renders the QR additionally as a PNG (`/tmp/wa-login-qr.png`, via the `qrcode` package) and opens it in a window (`feh --auto-zoom`, fallback `xdg-open`). Use when the terminal ASCII QR is hard to scan. `--force` wipes the old session first. The send path (`check-and-send.mjs`) hangs silently waiting for a QR if `whatsapp/auth/creds.json` is missing — check that file exists before any send and re-login via this script if not.
- `leave-group.mjs` — supports comma-separated JIDs for bulk-leave
- Session auth stored in `whatsapp/auth/` (excluded from git)
- First run requires QR code scan (WhatsApp → Linked Devices)
- Uses `fetchLatestBaileysVersion()` + `makeCacheableSignalKeyStore()` for stable connection
- Rust binary calls Node.js script as subprocess via `std::process::Command`; `find_node()` searches nvm, Homebrew, and system paths

## Pump Tsüri Welcome (`sync-contacts` / `welcome` alias)

End-to-end flow: Google Form (responses sheet) → SQLite → Baileys WhatsApp send, with a **Gmail-API e-mail fallback** for signups not reachable on WhatsApp (see `src/gmail.rs` below).

- **Auth**: service-account JSON at `whatsapp/google-sa.json` (gitignored). `src/google_sheets.rs` signs an RS256 JWT (`jsonwebtoken` crate), exchanges for an access token, then calls `spreadsheets.values.get`. No public sharing of the sheet — only the SA email needs Viewer access.
- **Sheet resolution**: input is a URL OR ID; `parse_sheet_id_and_gid()` extracts both. `resolve_sheet_title()` fetches metadata to map `gid` → sheet tab title (e.g. "Antwort", "Formularantworten 1"), then `fetch_values()` reads `<title>!A:Z`.
- **Presets**: `Commands::SyncContacts` takes an optional positional `variant`. Three `WelcomePreset` constants in `main.rs` bundle the per-variant defaults — sheet URL, DB filename, welcome text, default-image flag, and column letters. Each preset's defaults are applied via `unwrap_or_else` after clap parsing, so individual flags (`--sheet`, `--welcome`, `--mobile-col`, `--db`, …) still override on a per-field basis. Current presets:
  - `pumper` (default) — Pump-Tsüri signup, `contacts.db`, columns C/J/D, image on.
  - `pp` (power pumper, one-minute achievement) — different sheet, `contacts_pp.db`, columns D/C/B, image off, Twint/cap message.
  - `build` (build & pump event) — `contacts_build.db`, columns E/C/D (mobile/first/last), image off, English text-only welcome ("Welcome to the build and pump event {first}."). The signup sheet must be shared with the service-account email (`pegelstand@pegelstand.iam.gserviceaccount.com`) as Viewer or reads 403. To message only one new entry (e.g. just today's signup), `--mark-existing` to mark the whole pending set as greeted, `DELETE` that one row from `contacts`, then a normal run sends to it alone.
- **Storage**: `whatsapp/contacts*.db` (SQLite via `rusqlite` with `bundled` feature — no system dep). `db_path(name)` and `open_db(name)` take the filename so multiple variants stay isolated. Two tables per DB:
  - `submissions` — full raw archive. PK = `row_index` (1-based, header is row 1). `data` column is a JSON object keyed by header name (source of truth for any schema evolution). Plus one TEXT column per sheet header, added on the fly via `ALTER TABLE` when a new header appears, and backfilled from JSON for existing rows so old data is immediately queryable without `json_extract`. Sanitization: lowercase, non-alnum → `_`, capped at 50 chars, deduped on collision.
  - `contacts` — registry of WhatsApp-confirmed recipients. PK = `jid`. FK `row_index` → `submissions(row_index)`. Inserted with `INSERT OR IGNORE` only **after** Baileys confirms the send.
- **Phone normalization** (`normalize_phone()` in `sync_contacts.rs`):
  1. Token-extraction: scans the cell for `+digits` or bare-digit runs (≥7 digits) bounded by separators (space/dash/parens/dot/comma/slash). Letters and multi-byte punctuation terminate a token. Multiple tokens per cell are kept as candidates.
  2. Multi-`+` rule: if multiple `+`-prefixed candidates and the lowercase string contains "whatsapp", pick the candidate nearest (byte distance) to the word. Handles `+41 77 902 18 93 (Whatsapp: +48696905840)` → `+48696905840`.
  3. CC normalization: strip `00` → `+`; leading `0` → `+<default_cc>`; bare 9-digit Swiss starting with `7` → `+41…` heuristic.
  4. Length sanity: 10–15 digits; `+41` numbers must be exactly 11 digits after `+`.
- **Node helper**: `whatsapp/check-and-send.mjs` — reads job JSON `{contacts, welcome, imagePath}` from a temp file, connects via Baileys, calls `sock.onWhatsApp(jid)` per contact, sends image+caption (or text only if no `imagePath`), writes results JSON back. 1.5s pause between sends, 10s post-send delay for `creds.json` flush (same pattern as `send-doc.mjs`).
- **PNG generation**: by re-invoking `std::env::current_exe()` with `svg --start … --end … --png` rather than duplicating chart code. Expected output path: `png/zurichsee_{start}_{end}.png`. Skipped if preset has `default_image: false` OR `--no-image` passed (effective = `no_image || !preset.default_image`).
- **CLI flags**: positional `variant`, then `--sheet`, `--mobile-col`, `--first-col`, `--last-col`, `--welcome`, `--db` (all `Option<String>` — preset supplies defaults); plus `--cc 41`, `--days 3`, `--no-image`, `--dry-run`, `--mark-existing`, `--regen-docs`, `--regen-rows "132,135"`.
- **`--dry-run`** writes submissions (passive archive) but skips PNG generation, Node helper, and contacts insert.
- **`--mark-existing`** writes the current pending set straight into `contacts` without sending — one-shot backfill for an existing form population so the next run only messages genuine new submissions.
- **`--regen-docs`** (pp only) regenerates OneDrive Mütze documents for already-registered `contacts` **without** any WhatsApp send — recovery path when the OneDrive login failed on the original run, or to re-create docs. Queries `contacts` (first_name/last_name/row_index), looks up the address from the live sheet row, parses + replaces placeholders, uploads. `--regen-rows "132,135"` restricts to specific 1-based row indices; empty = all registered contacts. Branch returns early before the send/PNG path.
- **E-Mail-Fallback (`src/gmail.rs`)**: signups that are **not on WhatsApp** (`onWhatsApp` returns not-registered) get the welcome by e-mail instead — same text, with the Zürichsee PNG attached. Sent via the **Gmail API** (`users.messages.send`) from `zdavatz@gmail.com`. Auth uses **Application Default Credentials** (`~/.config/gcloud/application_default_credentials.json`, `authorized_user` format): `GmailSender::from_adc()` reads `client_id`/`client_secret`/`refresh_token`, refreshes against `oauth2.googleapis.com/token`, then POSTs a base64url'd MIME message (`multipart/mixed`: text part + base64 PNG part); `quota_project_id` → `x-goog-user-project` header. The e-mail column is auto-detected from the sheet header (any column whose name contains "mail"). The recipient's `Email` is added to `Pending`; after a successful send they are `insert_contact()`'d (marked greeted) exactly like a WhatsApp recipient, so they are never re-mailed. Per-preset `email_subject` (with `{first}` etc. placeholders) sets the subject line. **ADC setup gotcha**: the stock gcloud OAuth client (`764086051850-…`) is **not allowed to request `gmail.send`** — it silently drops it and yields `cloud-platform`/`sqlservice.login` scopes, so the send returns `403 ACCESS_TOKEN_SCOPE_INSUFFICIENT`. You must use your **own** OAuth Desktop client. Even `gcloud auth application-default login --client-id-file=… --scopes=…gmail.send` proved unreliable (kept reissuing the default scope set from a cached grant). The working path is a **direct OAuth loopback** with the project's Desktop client (`prompt=consent`, scope = exactly `gmail.send openid userinfo.email`) writing the ADC by hand. Gmail API must be enabled on the quota project (`gcloud services enable gmail.googleapis.com --project pegelstand`); `gmail.send` is a *sensitive* (not restricted) scope, so the OAuth consent screen in **Testing** mode with `zdavatz@gmail.com` as a test user works without app verification. Only dep needed is `base64` (already present). Token/credentials live outside the repo under `~/`; nothing secret is committed.
- **OneDrive (`src/onedrive.rs`)**: device-code OAuth against `/consumers/` (OneDrive Personal), token cached in `whatsapp/onedrive-token.json` (gitignored). The Azure app registration **must** support personal Microsoft accounts: `signInAudience = AzureADandPersonalMicrosoftAccount`, `api.requestedAccessTokenVersion = 2`, and `isFallbackPublicClient = true` (public client flows) — a single-tenant (`AzureADMyOrg`) app yields `AADSTS700016` on the consumers endpoint. PRESET_PP's `docx_template_id` is the **OneDrive-Personal item-ID** form (`8DB8718F73B2D606!s…`), NOT the `resid` GUID from the share/web URL — that GUID is unaddressable via Graph `/items/` ("ObjectHandle is Invalid") but resolvable via `GET /shares/u!<base64url(webUrl)>/driveItem`. Template `Vorname Nachname.docx` and the output live in `/Dokumente/wakethief`.
- **Tests**: `cargo test --release sync_contacts` — synthetic placeholder numbers (000-padded / US 555 fiction range) covering all parser branches. Never commit real subscriber numbers as test inputs.

## HTML Reports

The `report` command generates self-contained HTML files:
- **Default**: Chart.js (interactive, Canvas-based) — `include_str!("chartjs.min.js")` embeds the library at compile time
- **`--svg`**: Pure SVG charts generated in `svg_report.rs` — no JavaScript, works in WhatsApp/email/offline viewers
- **`--silvaplana`/`--neuenburgersee`/`--urnersee`/`--greifensee`/`--sihlsee`**: Lake-specific reports using MeteoSwiss SMN wind/weather data — auto InfluxDB fallback for >30 days
- **`--ermioni`**: Ermioni report using Open-Meteo weather + marine wave data — includes wave height chart
- Lake reports use a `LakeConfig` struct with station names, descriptions, lat/lon coordinates, and webcam links
- All reports include clickable Google Maps links to measurement stations (`target="_blank"`)
- All reports include webcam links per location (all `target="_blank" rel="noopener"`)
- Zürichsee modes merge Tiefenbrunnen + Mythenquai data and label every field with its source station (T/M)
- SVG charts: hex colors use a `hc()` helper to prepend `#` at runtime (because `"#..."` inside `r#""#` terminates the raw string)

## Rechtsgrundlagen-Dossier (`rechtsgrundlagen` binary)

Standalone binary `src/bin/rechtsgrundlagen.rs` (auto-discovered by Cargo, separate from the main `pegelstand` binary) that renders a legal-grounds PDF for pumpfoiling on the Zürichsee — the applicable laws (Bund/interkantonal/Kanton/Stadt) plus the AWEL "gelbe Zone"/Ausnahme situation. Companion document to Schriftliche Anfrage GR Nr. 2026/250.
- **Pure Rust, no Chrome**: built with `genpdf` (DejaVu Sans embedded for umlauts + «»„" quotes), same approach as listingtracker's `baugeschichte`.
- **Clickable links**: genpdf 0.2 can't emit hyperlinks, so `add_links()` reopens the finished PDF with `lopdf` and overlays `/Link` URI annotations. It locates every URL line by the **reserved link font size (9 pt, used for nothing else)**, walking the content stream across **all pages** in reading order (Td/Tm origins are plain numbers even though the glyphs are CID-encoded) and zipping the hits to the pushed-URL list in order. The clickable rect spans the full line width (works for left- and centre-aligned URLs).
- **genpdf gotcha**: a line whose single unbreakable "word" (e.g. a long URL) is wider than the column is **silently dropped**. So `linkline(display, url, …)` decouples the visible (short) text from the linked URL — long zhlex/gemeinderat URLs show a short label but the annotation carries the real URL. Keep the link font size unique or `add_links()` will mis-match.
- Deps added for this: `genpdf`, `lopdf`, `anyhow`. Font dir override: `FONT_DIR` (default `/usr/share/fonts/dejavu`). Output: `recht/Rechtsgrundlagen_Pumpfoiling_Zuerichsee.pdf`.
- Legal content was sourced from fedlex/zh.ch/stadt-zuerich.ch plus the AWEL/Wasserschutzpolizei/Sportamt e-mail correspondence (Pumpfoil = Schiff / "wettkampftaugliche Wassersportgeräte" per BSV Art. 134a; AWEL-Gebietsbetreuer Huber: temporäre Aufhebung der Sperrfläche 7–9 Uhr "möglicherweise rechtlich möglich").
- **Section 7.4 (Präzedenzfall Greifensee)**: documented case where a Ruderclub could not forbid using its Steg for pumpfoil practice; after a complaint the AWEL put up a Verbotstafel, but the Kanton's Rechtsdienst held there is *no* legal basis for any prohibition sign — the Steg is open to everyone (jedermann) and the Konzession grants the Ruderclub no Benutzungspriorität. Added as an `h2` + quote in `src/bin/rechtsgrundlagen.rs` before section 8; no new clickable link (still 17). Attribution left generic ("Rechtsdienst des Kantons Zürich") pending a named source/date.

## Bojendistanzmessung (`bojendistanz` binary)

Standalone binary `src/bin/bojendistanz.rs` (auto-discovered by Cargo) that turns one or more **u-blox GPS logs (CSV)** into a one-page PDF report with a map + per-measurement stats. Built for measuring buoy-to-buoy distances at Seebad Zollikon (a track between two buoys), but general.
- **CSV format**: u-blox `UbloxGps_*.csv` — columns `Time [10ms],UTC,Lat [deg],Lon [deg],Alt [m],SpeedKMh,Course [deg],Fix,NumSat,HDOP`. Rows are interleaved (some have speed, no alt; some the reverse) — all rows with valid lat/lon form the track; `SpeedKMh` where present gives min/max. `UTC` is `HHMMSS.ss`; local time = UTC+2 (MESZ). Date is parsed from the `_YYYYMMDD_HHMMSS` filename pattern (first matching file).
- **Multiple tracks**: pass several CSVs → several coloured tracks on one map. Track **endpoints are clustered into buoys** (`cluster_buoys()`, 14 m threshold, averaged, sorted N→S and numbered). Each track is labelled with the buoy pair it connects (`Boje a–b`). The report footer sums consecutive-buoy distances into a total **Bojenlinie** length — this is how three ~50–58 m segments reveal the full >100 m buoy line.
- **Basemap**: **Google Maps Static API satellite by default**, OSM tiles as fallback (`--osm`, or automatically if no key). Both use the same Web-Mercator projection (`lonlat_to_px`), so the scale bar, track and imagery are always mutually consistent — a single track just *is* short relative to the whole badi (Seebad swimming zone is ~86×166 m per OSM `swimming_area`), which is the entire point of the multi-track view.
  - **Google key**: read from `$GOOGLE_MAPS_STATIC_KEY` or `~/.config/pegelstand/maps-static-key.txt` (gitignored, **never committed**). Created for `zdavatz@gmail.com` in project **pegelstand** (billing already active): `gcloud services enable apikeys.googleapis.com static-maps-backend.googleapis.com --project pegelstand`, then `gcloud services api-keys create --project pegelstand --api-target=service=static-maps-backend.googleapis.com` (key restricted to the Static Maps API). The returned static image already carries the Google + imagery attribution — do not crop it; the scale bar sits bottom-left *above* the logo.
  - Google Static projection: point pixel = `lonlat_to_px(pt,z) - lonlat_to_px(center,z) + size/2`, then `× scale`. Fetched at `size=640, scale=2, maptype=satellite` (1280×1280 px). Zoom auto-picked as the highest where the combined bbox (+30 % margin) fits.
- **Rendering**: map built as SVG (basemap `<image>` data-URI + track polylines + buoy markers + scale bar + legend) → `resvg` → RGBA; alpha is stripped to **RGB** via the `image` crate because `printpdf`/`genpdf` reject alpha PNGs. PDF via `genpdf` (needs the **`images`** feature — enabled in `Cargo.toml`; added `image = "0.23"` as the flattening dep). Map fit to ~170 mm wide / ~132 mm tall so the stats block stays on the same page.
- **CLI**: `bojendistanz [csv...] [--osm] [--title <name>] [--out <pdf>]`. No CSV args → `DEFAULT_CSVS` (the three committed `messung/*.csv`). Output: `messung/Bojendistanz_<place>.pdf` + `messung/bojendistanz_map.png`. Committed sample data + PDF live in `messung/`.

## Build & Run

```bash
cargo build --release
./target/release/pegelstand zurichsee
./target/release/pegelstand report --start 2026-03-25 --end 2026-03-26 --svg
./target/release/pegelstand silvaplana --aktuell
./target/release/pegelstand report --start 2025-05-01 --end 2025-09-30 --silvaplana
./target/release/pegelstand ermioni --aktuell
./target/release/pegelstand report --start 2025-05-01 --end 2025-09-30 --ermioni
./target/release/pegelstand ermioni --start 2026-04-10 --end 2026-04-17 --png
./target/release/pegelstand ermioni --start 2026-04-10 --end 2026-04-17 --png --whatsapp "34635809989-1484605176@g.us"
./target/release/pegelstand svg --start 2026-04-05 --end 2026-04-10
./target/release/pegelstand svg --start 2026-04-10 --end 2026-04-11 --png
./target/release/pegelstand svg --start 2026-04-10 --end 2026-04-11 --png --whatsapp "34635809989-1484605176@g.us"
./target/release/pegelstand svg --start 2026-04-25 --end 2026-04-30 --png --bg ~/Pictures/foto.heic
./target/release/pegelstand ermioni --start 2026-04-25 --end 2026-04-30 --png --bg ~/Pictures/foto.heic
./target/release/pegelstand paleafokea
./target/release/pegelstand paleafokea --png
# Legal-grounds dossier (separate binary):
cargo run --release --bin rechtsgrundlagen   # → recht/Rechtsgrundlagen_Pumpfoiling_Zuerichsee.pdf
# Buoy-distance report from u-blox GPS logs (separate binary):
cargo run --release --bin bojendistanz                       # → messung/Bojendistanz_Seebad_Zollikon.pdf (default CSVs, Google satellite)
cargo run --release --bin bojendistanz -- a.csv b.csv c.csv  # custom logs
cargo run --release --bin bojendistanz -- a.csv --osm        # OSM basemap fallback (no Maps key)
```
