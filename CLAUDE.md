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

Code is split across:
- `src/main.rs` — CLI, API clients, all commands
- `src/svg_report.rs` — pure SVG chart generation (no JS dependencies)
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

5. **Poseidon/HCMR** — `https://api.poseidon.hcmr.gr/api`
   - Greek marine research buoys, OAuth2 auth required
   - Token via env vars: `POSEIDON_CLIENT_ID`, `POSEIDON_CLIENT_SECRET`
   - Saronikos buoy (~30 km NE of Ermioni): wind, waves, water temp, currents
   - Register: https://auth.poseidon.hcmr.gr/auth/register/

## Zürichsee Reglement 1977

The `zurichsee` command evaluates the current water level against the 1977 regulation:
- Regulierlinie varies by month (approximated from the regulation chart)
- Abflussgrenze (lower limit): 405.90 m ü.M.
- Critical high: > 407.50 m ü.M.

## Standalone SVG

The `svg` command generates a pure SVG file (no HTML wrapper) with Zürichsee Pegelstand, Wassertemperatur, and Lufttemperatur:
- Two charts: temperature (water + air) and water level
- Uses `write_standalone_svg()` in `svg_report.rs`
- Fetches Tecdottir Tiefenbrunnen (T) + Mythenquai (M), merges by timestamp
- Date format: dd.mm.yyyy throughout
- Default: last 5 days, output to `svg/` directory

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

## Build & Run

```bash
cargo build --release
./target/release/pegelstand zurichsee
./target/release/pegelstand report --start 2026-03-25 --end 2026-03-26 --svg
./target/release/pegelstand silvaplana --aktuell
./target/release/pegelstand report --start 2025-05-01 --end 2025-09-30 --silvaplana
./target/release/pegelstand ermioni --aktuell
./target/release/pegelstand report --start 2025-05-01 --end 2025-09-30 --ermioni
./target/release/pegelstand svg --start 2026-04-05 --end 2026-04-10
```
