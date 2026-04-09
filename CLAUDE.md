# CLAUDE.md

## Project Overview

Rust CLI tool (`pegelstand`) for querying Swiss water level and temperature data.

## Architecture

Single-binary CLI built with:
- `clap` for argument parsing (derive mode)
- `reqwest` for HTTP requests
- `serde` / `serde_json` for JSON deserialization
- `csv` for InfluxDB CSV response parsing
- `chrono` for date/time handling
- `tokio` for async runtime

Code is split across:
- `src/main.rs` — CLI, API clients, all commands
- `src/svg_report.rs` — pure SVG chart generation (no JS dependencies)
- `src/chartjs.min.js` — Chart.js library, embedded at compile time via `include_str!`

## APIs Used

1. **api.existenz.ch** (BAFU hydrology) — base URL: `https://api.existenz.ch/apiv1/hydro`
   - `/locations`, `/parameters`, `/latest` endpoints
   - Note: `LocationDetails.id` can be string or integer — requires custom deserializer (`string_or_int`)
   - Temperature data only available for rivers, not lakes

2. **InfluxDB** at `https://influx.konzept.space` — historical data via Flux queries
   - Read-only token is public (embedded in code)
   - Bucket: `existenzApi`, org: `api.existenz.ch`

3. **Tecdottir** (Stadt Zürich / Wasserschutzpolizei) — `https://tecdottir.metaodi.ch`
   - Zürichsee water temperature and weather at stations `tiefenbrunnen` and `mythenquai`
   - API limit: 1000 records per request, paginate with `offset`
   - `seetemperatur` and `report` commands merge both stations: Tiefenbrunnen provides temperature/wind/pressure, Mythenquai provides precipitation/radiation/water_level
   - All 14 fields: water_temperature, air_temperature, windchill, dew_point, humidity, wind_speed_avg_10min, wind_gust_max_10min, wind_force_avg_10min, wind_direction, barometric_pressure_qfe, precipitation, global_radiation, water_level

## Zürichsee Reglement 1977

The `zurichsee` command evaluates the current water level against the 1977 regulation:
- Regulierlinie varies by month (approximated from the regulation chart)
- Abflussgrenze (lower limit): 405.90 m ü.M.
- Critical high: > 407.50 m ü.M.

## HTML Reports

The `report` command generates self-contained HTML files:
- **Default**: Chart.js (interactive, Canvas-based) — `include_str!("chartjs.min.js")` embeds the library at compile time
- **`--svg`**: Pure SVG charts generated in `svg_report.rs` — no JavaScript, works in WhatsApp/email/offline viewers
- Both modes merge Tiefenbrunnen + Mythenquai data and label every field with its source station (T/M)
- SVG charts use `r#"..."#` raw strings avoided — hex colors use a `hc()` helper to prepend `#` at runtime (because `"#..."` inside `r#""#` terminates the raw string)

## Build & Run

```bash
cargo build --release
./target/release/pegelstand zurichsee
./target/release/pegelstand report --start 2026-03-25 --end 2026-03-26 --svg
```
