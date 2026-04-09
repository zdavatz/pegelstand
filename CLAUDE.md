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

All code is in `src/main.rs`.

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

## Zürichsee Reglement 1977

The `zurichsee` command evaluates the current water level against the 1977 regulation:
- Regulierlinie varies by month (approximated from the regulation chart)
- Abflussgrenze (lower limit): 405.90 m ü.M.
- Critical high: > 407.50 m ü.M.

## Build & Run

```bash
cargo build --release
./target/release/pegelstand zurichsee
```
