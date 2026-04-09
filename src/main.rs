mod svg_report;

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde::de::{self, Deserializer};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;
use std::io::Write as IoWrite;

/// Deserialize a value that might be a string or integer into a String.
fn string_or_int<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringOrInt;

    impl<'de> de::Visitor<'de> for StringOrInt {
        type Value = String;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or integer")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_owned())
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<String, E> {
            Ok(v.to_string())
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<String, E> {
            Ok(v.to_string())
        }
    }

    deserializer.deserialize_any(StringOrInt)
}

const BASE_URL: &str = "https://api.existenz.ch/apiv1/hydro";
const APP_NAME: &str = "pegelstand";
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

const SMN_URL: &str = "https://api.existenz.ch/apiv1/smn";

const POSEIDON_API: &str = "https://api.poseidon.hcmr.gr/api";
const POSEIDON_TOKEN_URL: &str = "https://auth.poseidon.hcmr.gr/o/token/";
const OPEN_METEO_URL: &str = "https://api.open-meteo.com/v1/forecast";
const OPEN_METEO_ARCHIVE: &str = "https://archive-api.open-meteo.com/v1/archive";
const OPEN_METEO_MARINE: &str = "https://marine-api.open-meteo.com/v1/marine";

const INFLUX_URL: &str = "https://influx.konzept.space";
const INFLUX_ORG: &str = "api.existenz.ch";
const INFLUX_TOKEN: &str = "0yLbh-D7RMe1sX1iIudFel8CcqCI8sVfuRTaliUp56MgE6kub8-nSd05_EJ4zTTKt0lUzw8zcO73zL9QhC3jtA==";

const TECDOTTIR_URL: &str = "https://tecdottir.metaodi.ch";

// --- API response types ---

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    #[allow(dead_code)]
    source: String,
    payload: T,
}

#[derive(Debug, Deserialize)]
struct LocationDetails {
    #[allow(dead_code)]
    #[serde(deserialize_with = "string_or_int")]
    id: String,
    name: String,
    #[serde(rename = "water-body-name")]
    water_body_name: String,
    #[serde(rename = "water-body-type")]
    water_body_type: String,
    lat: f64,
    lon: f64,
}

#[derive(Debug, Deserialize)]
struct Location {
    #[allow(dead_code)]
    name: String,
    details: LocationDetails,
}

#[derive(Debug, Deserialize)]
struct ParameterDetails {
    name: String,
}

#[derive(Debug, Deserialize)]
struct Parameter {
    name: String,
    unit: String,
    details: ParameterDetails,
}

// --- Tecdottir (Stadt Zürich) types ---

#[derive(Debug, Deserialize)]
struct TecdottirResponse {
    #[allow(dead_code)]
    ok: bool,
    result: Vec<TecdottirMeasurement>,
}

#[derive(Debug, Deserialize)]
struct TecdottirMeasurement {
    #[allow(dead_code)]
    station: String,
    timestamp: String,
    values: TecdottirValues,
}

#[derive(Debug, Deserialize)]
struct TecdottirValues {
    water_temperature: TecdottirValue,
    air_temperature: TecdottirValue,
    #[serde(default)]
    humidity: Option<TecdottirValue>,
    #[serde(default)]
    wind_speed_avg_10min: Option<TecdottirValue>,
    #[serde(default)]
    wind_gust_max_10min: Option<TecdottirValue>,
    #[serde(default)]
    wind_force_avg_10min: Option<TecdottirValue>,
    #[serde(default)]
    wind_direction: Option<TecdottirValue>,
    #[serde(default)]
    windchill: Option<TecdottirValue>,
    #[serde(default)]
    barometric_pressure_qfe: Option<TecdottirValue>,
    #[serde(default)]
    dew_point: Option<TecdottirValue>,
    #[serde(default)]
    precipitation: Option<TecdottirValue>,
    #[serde(default)]
    global_radiation: Option<TecdottirValue>,
    #[serde(default)]
    water_level: Option<TecdottirValue>,
}

#[derive(Debug, Deserialize, Default)]
struct TecdottirValue {
    value: Option<f64>,
    #[allow(dead_code)]
    unit: String,
}

// --- BAFU types ---

#[derive(Debug, Deserialize)]
struct Measurement {
    timestamp: i64,
    loc: String,
    par: String,
    val: f64,
}

// --- CLI ---

#[derive(Parser)]
#[command(name = "pegelstand", about = "Pegelstand-Abfrage via api.existenz.ch (BAFU)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Alle verfügbaren Messstationen anzeigen
    Locations {
        /// Filter nach Gewässername (z.B. "Zürichsee")
        #[arg(short, long)]
        filter: Option<String>,
    },
    /// Verfügbare Parameter anzeigen
    Parameters,
    /// Aktuelle Messwerte abrufen
    Latest {
        /// Stations-IDs, kommagetrennt (z.B. "2209,2135")
        #[arg(short, long)]
        locations: String,
        /// Parameter, kommagetrennt (z.B. "height,temperature,flow")
        #[arg(short, long, default_value = "height,temperature,flow")]
        parameters: String,
    },
    /// Zürichsee-Pegel mit Reglement-Bewertung
    Zurichsee,
    /// Historische Daten via InfluxDB (bis 2001 zurück)
    History {
        /// Stations-ID (z.B. "2209")
        #[arg(short, long)]
        location: String,
        /// Parameter (z.B. "height", "temperature", "flow")
        #[arg(short, long, default_value = "height")]
        parameter: String,
        /// Zeitraum zurück (z.B. "3mo", "1y", "30d")
        #[arg(short, long, default_value = "3mo")]
        range: String,
        /// Aggregation (z.B. "1d", "1h", "6h")
        #[arg(short, long, default_value = "1d")]
        every: String,
    },
    /// Aktuelle Wassertemperaturen aller Stationen
    Temperaturen {
        /// Sortierung: "temp" (Standard), "name", "gewaesser"
        #[arg(short, long, default_value = "temp")]
        sort: String,
        /// Filter nach Gewässername (z.B. "Aare", "Rhein")
        #[arg(short, long)]
        filter: Option<String>,
    },
    /// Zürichsee Wassertemperatur (Stadt Zürich / Wasserschutzpolizei)
    Seetemperatur {
        /// Station: "tiefenbrunnen" oder "mythenquai"
        #[arg(short = 'S', long, default_value = "tiefenbrunnen")]
        station: String,
        /// Startdatum (YYYY-MM-DD), Standard: vor 3 Monaten
        #[arg(long)]
        start: Option<String>,
        /// Enddatum (YYYY-MM-DD), Standard: heute
        #[arg(long)]
        end: Option<String>,
        /// Nur aktuellen Wert anzeigen (kein Verlauf)
        #[arg(long)]
        aktuell: bool,
        /// Alle 10-Minuten-Werte für einen Tag anzeigen (YYYY-MM-DD)
        #[arg(short, long)]
        datum: Option<String>,
    },
    /// Silvaplana: Wind, Wetter & Pegel (MeteoSwiss SIA + BAFU 2073)
    Silvaplana {
        /// Startdatum (YYYY-MM-DD), Standard: heute
        #[arg(long)]
        start: Option<String>,
        /// Enddatum (YYYY-MM-DD), Standard: heute
        #[arg(long)]
        end: Option<String>,
        /// Alle 10-Minuten-Werte für einen Tag (YYYY-MM-DD)
        #[arg(short, long)]
        datum: Option<String>,
        /// Nur aktuellen Wert anzeigen
        #[arg(long)]
        aktuell: bool,
    },
    /// Sihlsee: Wind, Wetter (MeteoSwiss EIN Einsiedeln)
    Sihlsee {
        /// Startdatum (YYYY-MM-DD), Standard: heute
        #[arg(long)]
        start: Option<String>,
        /// Enddatum (YYYY-MM-DD), Standard: heute
        #[arg(long)]
        end: Option<String>,
        /// Alle 10-Minuten-Werte für einen Tag (YYYY-MM-DD)
        #[arg(short, long)]
        datum: Option<String>,
        /// Nur aktuellen Wert anzeigen
        #[arg(long)]
        aktuell: bool,
    },
    /// Ermioni: Wind, Wetter & Wellen (Poseidon/HCMR + Open-Meteo)
    Ermioni {
        /// Startdatum (YYYY-MM-DD), Standard: vor 7 Tagen
        #[arg(long)]
        start: Option<String>,
        /// Enddatum (YYYY-MM-DD), Standard: heute
        #[arg(long)]
        end: Option<String>,
        /// Nur aktuellen Wert anzeigen
        #[arg(long)]
        aktuell: bool,
    },
    /// HTML-Report generieren (Zürichsee, beide Stationen kombiniert)
    Report {
        /// Startdatum (YYYY-MM-DD)
        #[arg(long)]
        start: String,
        /// Enddatum (YYYY-MM-DD)
        #[arg(long)]
        end: String,
        /// Ausgabedatei (Standard: html/{start}_{end}.html)
        #[arg(short, long)]
        output: Option<String>,
        /// SVG-Charts statt Chart.js (kein JS, WhatsApp/Mail-kompatibel)
        #[arg(long)]
        svg: bool,
        /// Silvaplana-Report (MeteoSwiss SIA + BAFU 2073)
        #[arg(long)]
        silvaplana: bool,
        /// Neuenburgersee-Report (MeteoSwiss PAY + BAFU 2154)
        #[arg(long)]
        neuenburgersee: bool,
        /// Urnersee-Report (MeteoSwiss ALT + BAFU 2025)
        #[arg(long)]
        urnersee: bool,
        /// Greifensee-Report (MeteoSwiss PFA Pfaffikon ZH + BAFU 2082)
        #[arg(long)]
        greifensee: bool,
        /// Sihlsee-Report (MeteoSwiss EIN Einsiedeln)
        #[arg(long)]
        sihlsee: bool,
        /// Ermioni-Report (Open-Meteo + Poseidon/HCMR)
        #[arg(long)]
        ermioni: bool,
    },
}

// --- Zürichsee Reglement 1977 ---

struct ReglementBewertung {
    #[allow(dead_code)]
    pegel: f64,
    status: &'static str,
    beschreibung: String,
}

fn bewerte_zurichsee_pegel(pegel: f64, monat: u32) -> ReglementBewertung {
    // Regulierlinie variiert je nach Monat (approximiert aus dem Reglement-Diagramm 1977)
    let regulierlinie = match monat {
        1 => 406.00,
        2 => 406.00,
        3 => 406.05,
        4 => 406.10,
        5 => 406.20,
        6 => 406.30,
        7 => 406.25,
        8 => 406.15,
        9 => 406.05,
        10 => 406.00,
        11 => 406.00,
        12 => 406.00,
        _ => 406.00,
    };

    let abflussgrenze = 405.90;

    if pegel > 407.50 {
        ReglementBewertung {
            pegel,
            status: "KRITISCH HOCH",
            beschreibung: format!(
                "Pegel {:.2} m liegt deutlich über Regulierlinie ({:.2} m) — Hochwassergefahr!",
                pegel, regulierlinie
            ),
        }
    } else if pegel > regulierlinie + 0.30 {
        ReglementBewertung {
            pegel,
            status: "HOCH",
            beschreibung: format!(
                "Pegel {:.2} m liegt {:.2} m über Regulierlinie ({:.2} m)",
                pegel,
                pegel - regulierlinie,
                regulierlinie
            ),
        }
    } else if pegel > regulierlinie {
        ReglementBewertung {
            pegel,
            status: "LEICHT ERHÖHT",
            beschreibung: format!(
                "Pegel {:.2} m liegt {:.2} m über Regulierlinie ({:.2} m)",
                pegel,
                pegel - regulierlinie,
                regulierlinie
            ),
        }
    } else if pegel >= abflussgrenze {
        ReglementBewertung {
            pegel,
            status: "NORMAL",
            beschreibung: format!(
                "Pegel {:.2} m im Normalbereich (Regulierlinie: {:.2} m, Abflussgrenze: {:.2} m)",
                pegel, regulierlinie, abflussgrenze
            ),
        }
    } else {
        ReglementBewertung {
            pegel,
            status: "TIEF",
            beschreibung: format!(
                "Pegel {:.2} m liegt unter der Abflussgrenze ({:.2} m)",
                pegel, abflussgrenze
            ),
        }
    }
}

// --- HTTP helpers ---

fn build_url(endpoint: &str, params: &[(&str, &str)]) -> String {
    let mut url = format!("{}/{}", BASE_URL, endpoint);
    let mut all_params: Vec<(&str, &str)> = vec![("app", APP_NAME), ("version", APP_VERSION)];
    all_params.extend_from_slice(params);

    url.push('?');
    let query: Vec<String> = all_params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    url.push_str(&query.join("&"));
    url
}

async fn fetch_locations(
    client: &reqwest::Client,
) -> Result<HashMap<String, Location>, Box<dyn std::error::Error>> {
    let url = build_url("locations", &[]);
    let resp: ApiResponse<HashMap<String, Location>> = client.get(&url).send().await?.json().await?;
    Ok(resp.payload)
}

async fn fetch_parameters(
    client: &reqwest::Client,
) -> Result<HashMap<String, Parameter>, Box<dyn std::error::Error>> {
    let url = build_url("parameters", &[]);
    let resp: ApiResponse<HashMap<String, Parameter>> =
        client.get(&url).send().await?.json().await?;
    Ok(resp.payload)
}

async fn fetch_latest(
    client: &reqwest::Client,
    locations: &str,
    parameters: &str,
) -> Result<Vec<Measurement>, Box<dyn std::error::Error>> {
    let url = build_url(
        "latest",
        &[("locations", locations), ("parameters", parameters)],
    );
    let resp: ApiResponse<Vec<Measurement>> = client.get(&url).send().await?.json().await?;
    Ok(resp.payload)
}

fn format_timestamp(ts: i64) -> String {
    DateTime::from_timestamp(ts, 0)
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
        .format("%d.%m.%Y %H:%M UTC")
        .to_string()
}

fn par_label(par: &str) -> &str {
    match par {
        "height" => "Pegel",
        "flow" => "Abfluss",
        "temperature" => "Temperatur",
        "flow_ls" => "Abfluss",
        "height_abs" => "Pegel (abs)",
        "oxygen" => "Sauerstoff",
        "conductivity" => "Leitfähigkeit",
        "acidity" => "Säuregehalt",
        "turbidity" => "Trübung",
        _ => par,
    }
}

fn par_unit(par: &str) -> &str {
    match par {
        "height" => "m ü.M.",
        "flow" => "m³/s",
        "temperature" => "°C",
        "flow_ls" => "l/s",
        "height_abs" => "m",
        "oxygen" => "mg/l",
        "conductivity" => "µS/cm",
        "acidity" => "pH",
        "turbidity" => "BSTU",
        _ => "",
    }
}

// --- InfluxDB ---

#[derive(Debug)]
struct HistoryPoint {
    time: String,
    value: f64,
}

async fn fetch_history(
    client: &reqwest::Client,
    location: &str,
    parameter: &str,
    range: &str,
    every: &str,
) -> Result<Vec<HistoryPoint>, Box<dyn std::error::Error>> {
    let flux = format!(
        r#"from(bucket: "existenzApi")
    |> range(start: -{range})
    |> filter(fn: (r) => r["_measurement"] == "hydro")
    |> filter(fn: (r) => r["_field"] == "{parameter}")
    |> filter(fn: (r) => r["loc"] == "{location}")
    |> aggregateWindow(every: {every}, fn: mean, createEmpty: false)
    |> yield(name: "mean")"#,
        range = range,
        parameter = parameter,
        location = location,
        every = every,
    );

    let url = format!("{}/api/v2/query?org={}", INFLUX_URL, INFLUX_ORG);
    let resp = client
        .post(&url)
        .header("Authorization", format!("Token {}", INFLUX_TOKEN))
        .header("Content-Type", "application/vnd.flux")
        .header("Accept", "application/csv")
        .body(flux)
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await?;

    if !status.is_success() {
        return Err(format!("InfluxDB Fehler ({}): {}", status, body).into());
    }

    let mut points = Vec::new();
    let mut rdr = csv::Reader::from_reader(body.as_bytes());

    // Find column indices from headers
    let headers = rdr.headers()?.clone();
    let time_idx = headers.iter().position(|h| h == "_time");
    let value_idx = headers.iter().position(|h| h == "_value");

    if let (Some(ti), Some(vi)) = (time_idx, value_idx) {
        for result in rdr.records() {
            let record = result?;
            if let (Some(time_str), Some(val_str)) = (record.get(ti), record.get(vi)) {
                if let Ok(val) = val_str.parse::<f64>() {
                    // Parse ISO time to nice format
                    let nice_time = if let Ok(dt) = time_str.parse::<DateTime<Utc>>() {
                        dt.format("%d.%m.%Y %H:%M").to_string()
                    } else {
                        time_str.to_string()
                    };
                    points.push(HistoryPoint {
                        time: nice_time,
                        value: val,
                    });
                }
            }
        }
    }

    Ok(points)
}

// --- Formatting helpers ---

fn fmt_opt_f1(v: Option<f64>) -> String {
    match v {
        Some(x) if !x.is_nan() => format!("{:.1}", x),
        _ => "-".into(),
    }
}

fn fmt_opt_f0(v: Option<f64>) -> String {
    match v {
        Some(x) if !x.is_nan() => format!("{:.0}", x),
        _ => "-".into(),
    }
}

fn wind_direction_label(deg: f64) -> &'static str {
    match ((deg + 22.5) % 360.0 / 45.0) as u32 {
        0 => "N",
        1 => "NO",
        2 => "O",
        3 => "SO",
        4 => "S",
        5 => "SW",
        6 => "W",
        7 => "NW",
        _ => "?",
    }
}

fn print_opt_src(label: &str, val: &Option<TecdottirValue>, unit: &str, decimals: u8, src: &str) {
    if let Some(v) = val {
        if let Some(x) = v.value {
            match decimals {
                0 => println!("  {:<20} {:>6.0} {}  ({})", label, x, unit, src),
                2 => println!("  {:<20} {:>6.2} {}  ({})", label, x, unit, src),
                _ => println!("  {:<20} {:>6.1} {}  ({})", label, x, unit, src),
            }
        }
    }
}



// --- Tecdottir (Zürichsee Temperatur) ---

// --- SMN (SwissMetNet) ---

const SMN_PARAMS: &str = "tt,td,rr,ss,rad,rh,dd,ff,fx,qfe";

async fn fetch_smn_latest(
    client: &reqwest::Client,
    location: &str,
) -> Result<Vec<Measurement>, Box<dyn std::error::Error>> {
    let url = format!(
        "{}/latest?locations={}&parameters={}&app={}&version={}",
        SMN_URL, location, SMN_PARAMS, APP_NAME, APP_VERSION
    );
    let resp: ApiResponse<Vec<Measurement>> = client.get(&url).send().await?.json().await?;
    Ok(resp.payload)
}

async fn fetch_smn_daterange(
    client: &reqwest::Client,
    location: &str,
    start: &str,
    end: &str,
) -> Result<Vec<Measurement>, Box<dyn std::error::Error>> {
    let url = format!(
        "{}/daterange?locations={}&parameters={}&startdate={}T00:00:00Z&enddate={}T23:59:59Z&app={}&version={}",
        SMN_URL, location, SMN_PARAMS, start, end, APP_NAME, APP_VERSION
    );
    let resp: ApiResponse<Vec<Measurement>> = client.get(&url).send().await?.json().await?;
    Ok(resp.payload)
}

fn smn_label(par: &str) -> &str {
    match par {
        "tt" => "Temperatur",
        "td" => "Taupunkt",
        "rr" => "Niederschlag",
        "ss" => "Sonne",
        "rad" => "Strahlung",
        "rh" => "Feuchtigkeit",
        "dd" => "Windrichtung",
        "ff" => "Wind",
        "fx" => "Böen",
        "qfe" => "Luftdruck",
        _ => par,
    }
}

fn smn_unit(par: &str) -> &str {
    match par {
        "tt" | "td" => "°C",
        "rr" => "mm",
        "ss" => "min",
        "rad" => "W/m²",
        "rh" => "%",
        "dd" => "°",
        "ff" | "fx" => "km/h",
        "qfe" => "hPa",
        _ => "",
    }
}

// --- Open-Meteo (Ermioni) ---

#[derive(Debug, Deserialize)]
struct OpenMeteoResponse {
    #[serde(default)]
    current: Option<OpenMeteoCurrent>,
    #[serde(default)]
    hourly: Option<OpenMeteoHourly>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoCurrent {
    #[serde(default)]
    temperature_2m: Option<f64>,
    #[serde(default)]
    wind_speed_10m: Option<f64>,
    #[serde(default)]
    wind_direction_10m: Option<f64>,
    #[serde(default)]
    wind_gusts_10m: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoHourly {
    time: Vec<String>,
    #[serde(default)]
    temperature_2m: Option<Vec<Option<f64>>>,
    #[serde(default)]
    wind_speed_10m: Option<Vec<Option<f64>>>,
    #[serde(default)]
    wind_direction_10m: Option<Vec<Option<f64>>>,
    #[serde(default)]
    wind_gusts_10m: Option<Vec<Option<f64>>>,
    #[serde(default)]
    relative_humidity_2m: Option<Vec<Option<f64>>>,
    #[serde(default)]
    pressure_msl: Option<Vec<Option<f64>>>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoMarineResponse {
    #[serde(default)]
    hourly: Option<OpenMeteoMarineHourly>,
}

#[derive(Debug, Deserialize)]
struct OpenMeteoMarineHourly {
    #[allow(dead_code)]
    time: Vec<String>,
    #[serde(default)]
    wave_height: Option<Vec<Option<f64>>>,
    #[serde(default)]
    wind_wave_direction: Option<Vec<Option<f64>>>,
    #[serde(default)]
    wind_wave_period: Option<Vec<Option<f64>>>,
}

const ERMIONI_LAT: f64 = 37.38;
const ERMIONI_LON: f64 = 23.25;

async fn fetch_open_meteo_current(
    client: &reqwest::Client,
) -> Result<OpenMeteoResponse, Box<dyn std::error::Error>> {
    let url = format!(
        "{}?latitude={}&longitude={}&current=temperature_2m,wind_speed_10m,wind_direction_10m,wind_gusts_10m&timezone=Europe/Athens",
        OPEN_METEO_URL, ERMIONI_LAT, ERMIONI_LON
    );
    let resp: OpenMeteoResponse = client.get(&url).send().await?.json().await?;
    Ok(resp)
}

async fn fetch_open_meteo_hourly(
    client: &reqwest::Client,
    start: &str,
    end: &str,
    is_archive: bool,
) -> Result<OpenMeteoResponse, Box<dyn std::error::Error>> {
    let base = if is_archive { OPEN_METEO_ARCHIVE } else { OPEN_METEO_URL };
    let mut url = format!(
        "{}?latitude={}&longitude={}&hourly=temperature_2m,wind_speed_10m,wind_direction_10m,wind_gusts_10m,relative_humidity_2m,pressure_msl&timezone=Europe/Athens",
        base, ERMIONI_LAT, ERMIONI_LON
    );
    if is_archive {
        url.push_str(&format!("&start_date={}&end_date={}", start, end));
    } else {
        url.push_str(&format!("&start_date={}&end_date={}", start, end));
    }
    let resp: OpenMeteoResponse = client.get(&url).send().await?.json().await?;
    Ok(resp)
}

async fn fetch_open_meteo_marine(
    client: &reqwest::Client,
    start: &str,
    end: &str,
) -> Result<OpenMeteoMarineResponse, Box<dyn std::error::Error>> {
    let url = format!(
        "{}?latitude={}&longitude={}&hourly=wave_height,wind_wave_direction,wind_wave_period&timezone=Europe/Athens&start_date={}&end_date={}",
        OPEN_METEO_MARINE, ERMIONI_LAT, ERMIONI_LON, start, end
    );
    let resp: OpenMeteoMarineResponse = client.get(&url).send().await?.json().await?;
    Ok(resp)
}

// --- Poseidon API (HCMR) ---

async fn fetch_poseidon_token(
    client: &reqwest::Client,
    username: &str,
    password: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Try password grant first (username/password login)
    let params = [
        ("grant_type", "password"),
        ("username", username),
        ("password", password),
    ];
    let resp: serde_json::Value = client
        .post(POSEIDON_TOKEN_URL)
        .form(&params)
        .send()
        .await?
        .json()
        .await?;

    if let Some(token) = resp["access_token"].as_str() {
        return Ok(token.to_string());
    }

    // Fallback: try client_credentials grant
    let params2 = [
        ("grant_type", "client_credentials"),
        ("client_id", username),
        ("client_secret", password),
    ];
    let resp2: serde_json::Value = client
        .post(POSEIDON_TOKEN_URL)
        .form(&params2)
        .send()
        .await?
        .json()
        .await?;

    resp2["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Poseidon token error: password={}, client_credentials={}", resp, resp2).into())
}

async fn fetch_poseidon_platforms(
    client: &reqwest::Client,
    token: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let url = format!("{}/platforms/", POSEIDON_API);
    let resp: serde_json::Value = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?
        .json()
        .await?;
    Ok(resp)
}

async fn fetch_poseidon_data(
    client: &reqwest::Client,
    token: &str,
    platform: &str,
    params: &str,
    start: &str,
    end: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let url = format!(
        "{}/data/{}/?param__pname__in={}&dt__gte={} 00:00:00&dt__lte={} 23:59:59&limit=1000&ordering=-dt",
        POSEIDON_API, platform, params, start, end
    );
    let resp: serde_json::Value = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await?
        .json()
        .await?;
    Ok(resp)
}

// --- Main ---

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    match cli.command {
        Commands::Locations { filter } => {
            let locations = fetch_locations(&client).await?;
            let mut entries: Vec<_> = locations.iter().collect();
            entries.sort_by_key(|(k, _)| k.to_string());

            println!(
                "{:<6} {:<25} {:<25} {:<8} {:>9} {:>9}",
                "ID", "Name", "Gewässer", "Typ", "Lat", "Lon"
            );
            println!("{}", "-".repeat(85));

            for (id, loc) in &entries {
                let d = &loc.details;
                if let Some(ref f) = filter {
                    let f_lower = f.to_lowercase();
                    if !d.water_body_name.to_lowercase().contains(&f_lower)
                        && !d.name.to_lowercase().contains(&f_lower)
                    {
                        continue;
                    }
                }
                println!(
                    "{:<6} {:<25} {:<25} {:<8} {:>9.4} {:>9.4}",
                    id, d.name, d.water_body_name, d.water_body_type, d.lat, d.lon
                );
            }
        }

        Commands::Parameters => {
            let params = fetch_parameters(&client).await?;
            let mut entries: Vec<_> = params.iter().collect();
            entries.sort_by_key(|(k, _)| k.to_string());

            println!(
                "{:<15} {:<30} {:<10}",
                "Parameter", "Beschreibung", "Einheit"
            );
            println!("{}", "-".repeat(55));
            for (_, p) in &entries {
                println!("{:<15} {:<30} {:<10}", p.name, p.details.name, p.unit);
            }
        }

        Commands::Latest {
            locations,
            parameters,
        } => {
            let locations_map = fetch_locations(&client).await?;
            let measurements = fetch_latest(&client, &locations, &parameters).await?;

            if measurements.is_empty() {
                println!("Keine Messwerte gefunden.");
                return Ok(());
            }

            // Group by location
            let mut by_loc: HashMap<String, Vec<&Measurement>> = HashMap::new();
            for m in &measurements {
                by_loc.entry(m.loc.clone()).or_default().push(m);
            }

            let mut loc_ids: Vec<_> = by_loc.keys().cloned().collect();
            loc_ids.sort();

            for loc_id in &loc_ids {
                let measures = &by_loc[loc_id];
                let loc_name = locations_map
                    .get(loc_id.as_str())
                    .map(|l| format!("{} ({})", l.details.name, l.details.water_body_name))
                    .unwrap_or_else(|| loc_id.clone());

                println!("\n Station {}: {}", loc_id, loc_name);
                if let Some(m) = measures.first() {
                    println!("  Zeitpunkt: {}", format_timestamp(m.timestamp));
                }
                println!("  {}", "-".repeat(40));
                for m in measures {
                    println!(
                        "  {:<15} {:>10.2} {}",
                        par_label(&m.par),
                        m.val,
                        par_unit(&m.par)
                    );
                }
            }
            println!();
        }

        Commands::History {
            location,
            parameter,
            range,
            every,
        } => {
            let locations_map = fetch_locations(&client).await?;
            let loc_name = locations_map
                .get(location.as_str())
                .map(|l| format!("{} ({})", l.details.name, l.details.water_body_name))
                .unwrap_or_else(|| location.clone());

            println!(
                "\n Historische Daten — Station {}: {}",
                location, loc_name
            );
            println!(
                "  Parameter: {}  |  Zeitraum: {}  |  Aggregation: {} (Mittelwert)",
                parameter, range, every
            );
            println!("  Quelle: BAFU via InfluxDB\n");

            let points = fetch_history(&client, &location, &parameter, &range, &every).await?;

            if points.is_empty() {
                println!("  Keine Daten gefunden.");
                return Ok(());
            }

            let unit = par_unit(&parameter);
            println!("  {:<18} {:>10} {}", "Datum", "Wert", unit);
            println!("  {}", "-".repeat(35));

            let min = points
                .iter()
                .map(|p| p.value)
                .fold(f64::INFINITY, f64::min);
            let max = points
                .iter()
                .map(|p| p.value)
                .fold(f64::NEG_INFINITY, f64::max);
            let avg = points.iter().map(|p| p.value).sum::<f64>() / points.len() as f64;

            for p in &points {
                println!("  {:<18} {:>10.2} {}", p.time, p.value, unit);
            }

            println!("\n  {}", "-".repeat(35));
            println!("  {:<18} {:>10.2} {}", "Minimum", min, unit);
            println!("  {:<18} {:>10.2} {}", "Maximum", max, unit);
            println!("  {:<18} {:>10.2} {}", "Durchschnitt", avg, unit);
            println!("  Datenpunkte: {}", points.len());
            println!();
        }

        Commands::Zurichsee => {
            let measurements =
                fetch_latest(&client, "2209", "height,temperature,flow").await?;

            let now = Utc::now();
            let monat = now.format("%m").to_string().parse::<u32>().unwrap_or(1);

            println!("\n Zürichsee — Pegel Zürich (Station 2209)");
            println!("  Quelle: BAFU / api.existenz.ch");

            if let Some(m) = measurements.first() {
                println!("  Zeitpunkt: {}", format_timestamp(m.timestamp));
            }
            println!("  {}", "-".repeat(45));

            let mut pegel_wert: Option<f64> = None;
            for m in &measurements {
                if m.par == "height" {
                    pegel_wert = Some(m.val);
                }
                println!(
                    "  {:<15} {:>10.2} {}",
                    par_label(&m.par),
                    m.val,
                    par_unit(&m.par)
                );
            }

            if let Some(pegel) = pegel_wert {
                let bewertung = bewerte_zurichsee_pegel(pegel, monat);
                println!();
                println!("  Reglement 1977: [{}]", bewertung.status);
                println!("  {}", bewertung.beschreibung);
            }
            println!();
        }

        Commands::Temperaturen { sort, filter } => {
            let (locations_map, measurements) = tokio::try_join!(
                fetch_locations(&client),
                fetch_latest(&client, "", "temperature"),
            )?;

            if measurements.is_empty() {
                println!("Keine Temperaturdaten gefunden.");
                return Ok(());
            }

            // Build display rows: (station_name, water_body, water_type, temp, timestamp)
            let mut rows: Vec<(String, String, String, f64, i64)> = Vec::new();
            for m in &measurements {
                let (name, water, wtype) = locations_map
                    .get(m.loc.as_str())
                    .map(|l| {
                        (
                            l.details.name.clone(),
                            l.details.water_body_name.clone(),
                            l.details.water_body_type.clone(),
                        )
                    })
                    .unwrap_or_else(|| (m.loc.clone(), "?".into(), "?".into()));

                if let Some(ref f) = filter {
                    let f_lower = f.to_lowercase();
                    if !water.to_lowercase().contains(&f_lower)
                        && !name.to_lowercase().contains(&f_lower)
                    {
                        continue;
                    }
                }

                rows.push((name, water, wtype, m.val, m.timestamp));
            }

            match sort.as_str() {
                "name" => rows.sort_by(|a, b| a.0.cmp(&b.0)),
                "gewaesser" => rows.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0))),
                _ => rows.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal)),
            }

            println!(
                "\n Wassertemperaturen — {} Stationen",
                rows.len()
            );
            if let Some(m) = measurements.first() {
                println!("  Zeitpunkt: {}", format_timestamp(m.timestamp));
            }
            println!("  Quelle: BAFU / api.existenz.ch\n");

            println!(
                "  {:<25} {:<25} {:<8} {:>8}",
                "Station", "Gewässer", "Typ", "°C"
            );
            println!("  {}", "-".repeat(70));

            for (name, water, wtype, temp, _) in &rows {
                println!(
                    "  {:<25} {:<25} {:<8} {:>8.1}",
                    name, water, wtype, temp
                );
            }
            println!();
        }

        Commands::Seetemperatur {
            station,
            start,
            end,
            aktuell,
            datum,
        } => {
            let now = Utc::now();
            let end_date = end.unwrap_or_else(|| now.format("%Y-%m-%d").to_string());

            if let Some(ref tag) = datum {
                // All 10-minute data points for a single day — merge both stations
                let next_day = chrono::NaiveDate::parse_from_str(tag, "%Y-%m-%d")
                    .map(|d| d.succ_opt().unwrap_or(d).format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|_| tag.clone());

                let url_tb = format!(
                    "{}/measurements/tiefenbrunnen?startDate={}&endDate={}&sort=timestamp_cet%20asc&limit=1000",
                    TECDOTTIR_URL, tag, next_day
                );
                let url_mq = format!(
                    "{}/measurements/mythenquai?startDate={}&endDate={}&sort=timestamp_cet%20asc&limit=1000",
                    TECDOTTIR_URL, tag, next_day
                );

                let (resp_tb, resp_mq) = tokio::try_join!(
                    async { client.get(&url_tb).send().await?.json::<TecdottirResponse>().await },
                    async { client.get(&url_mq).send().await?.json::<TecdottirResponse>().await },
                )?;

                // Index mythenquai by timestamp for merging
                let mq_by_time: HashMap<String, &TecdottirMeasurement> = resp_mq
                    .result
                    .iter()
                    .map(|m| {
                        let key = if m.timestamp.len() >= 16 {
                            m.timestamp[11..16].to_string()
                        } else {
                            m.timestamp.clone()
                        };
                        (key, m)
                    })
                    .collect();

                println!(
                    "\n Zürichsee — alle Messwerte {}",
                    tag
                );
                println!("  Quellen: Tiefenbrunnen (T) + Mythenquai (M) — Wasserschutzpolizei Zürich");
                println!("  Felder: Wasser/Luft/Chill/Tau/Feuchte/Wind/Böen/Bft/Richtung = T, Regen/Sonne/Pegel = M\n");

                if resp_tb.result.is_empty() {
                    println!("  Keine Daten für diesen Tag.");
                    return Ok(());
                }

                println!(
                    "  {:<6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>5} {:>7}",
                    "Zeit", "Wass.", "Luft", "Chill", "Tau", "Feu%", "Wind", "Böen", "Bft", "Ri°", "Druck", "Regen", "Sonn", "Pegel"
                );
                println!(
                    "  {:<6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>5} {:>7}",
                    "", "T °C", "T °C", "T °C", "T°C", "T %", "T m/s", "T", "T", "T", "T hPa", "M mm", "M W/m²", "M m"
                );
                println!("  {}", "-".repeat(102));

                let mut min_w = f64::INFINITY;
                let mut max_w = f64::NEG_INFINITY;
                let mut sum_w = 0.0;
                let mut count = 0usize;

                for m in &resp_tb.result {
                    let time = if m.timestamp.len() >= 16 {
                        m.timestamp[11..16].to_string()
                    } else {
                        m.timestamp.clone()
                    };

                    let v = &m.values;
                    let wt = v.water_temperature.value.unwrap_or(f64::NAN);
                    let at = v.air_temperature.value.unwrap_or(f64::NAN);
                    let wc = v.windchill.as_ref().and_then(|x| x.value);
                    let dp = v.dew_point.as_ref().and_then(|x| x.value);
                    let hu = v.humidity.as_ref().and_then(|x| x.value);
                    let ws = v.wind_speed_avg_10min.as_ref().and_then(|x| x.value);
                    let wg = v.wind_gust_max_10min.as_ref().and_then(|x| x.value);
                    let wf = v.wind_force_avg_10min.as_ref().and_then(|x| x.value);
                    let wd = v.wind_direction.as_ref().and_then(|x| x.value);
                    let bp = v.barometric_pressure_qfe.as_ref().and_then(|x| x.value);

                    // Precipitation, radiation, water_level from Mythenquai
                    let (pr, gr, wl) = if let Some(mq) = mq_by_time.get(&time) {
                        let mv = &mq.values;
                        (
                            mv.precipitation.as_ref().and_then(|x| x.value),
                            mv.global_radiation.as_ref().and_then(|x| x.value),
                            mv.water_level.as_ref().and_then(|x| x.value),
                        )
                    } else {
                        (None, None, None)
                    };

                    println!(
                        "  {:<6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>5} {:>7}",
                        time,
                        fmt_opt_f1(Some(wt)),
                        fmt_opt_f1(Some(at)),
                        fmt_opt_f1(wc),
                        fmt_opt_f1(dp),
                        fmt_opt_f0(hu),
                        fmt_opt_f1(ws),
                        fmt_opt_f1(wg),
                        fmt_opt_f0(wf),
                        fmt_opt_f0(wd),
                        fmt_opt_f0(bp),
                        fmt_opt_f1(pr),
                        fmt_opt_f0(gr),
                        fmt_opt_f1(wl),
                    );

                    if !wt.is_nan() {
                        if wt < min_w { min_w = wt; }
                        if wt > max_w { max_w = wt; }
                        sum_w += wt;
                        count += 1;
                    }
                }

                if count > 0 {
                    let avg_w = sum_w / count as f64;
                    println!("\n  {}", "-".repeat(102));
                    println!(
                        "  Wassertemperatur (T): Min {:.1}°C | Max {:.1}°C | Durchschnitt {:.1}°C",
                        min_w, max_w, avg_w
                    );
                    println!("  Messpunkte: {} (alle 10 Minuten)", count);
                    println!("  T = Tiefenbrunnen, M = Mythenquai");
                }
                println!();
            } else if aktuell {
                // Fetch latest from both stations
                let start_date = (now - chrono::Duration::days(2))
                    .format("%Y-%m-%d")
                    .to_string();
                let url_tb = format!(
                    "{}/measurements/tiefenbrunnen?startDate={}&endDate={}&sort=timestamp_cet%20desc&limit=1",
                    TECDOTTIR_URL, start_date, end_date
                );
                let url_mq = format!(
                    "{}/measurements/mythenquai?startDate={}&endDate={}&sort=timestamp_cet%20desc&limit=1",
                    TECDOTTIR_URL, start_date, end_date
                );

                let (resp_tb, resp_mq) = tokio::try_join!(
                    async { client.get(&url_tb).send().await?.json::<TecdottirResponse>().await },
                    async { client.get(&url_mq).send().await?.json::<TecdottirResponse>().await },
                )?;

                println!("\n Zürichsee — aktuelle Messwerte");
                println!("  Quellen: Tiefenbrunnen (T) + Mythenquai (M)\n");

                if let Some(tb) = resp_tb.result.last() {
                    let v = &tb.values;
                    println!("  Zeitpunkt:          {}", &tb.timestamp[..19]);
                    println!("  {}", "-".repeat(45));
                    println!("  {:<20} {:>6.1} °C  (T)", "Wassertemp:", v.water_temperature.value.unwrap_or(f64::NAN));
                    println!("  {:<20} {:>6.1} °C  (T)", "Lufttemp:", v.air_temperature.value.unwrap_or(f64::NAN));
                    print_opt_src("Windchill:", &v.windchill, "°C", 1, "T");
                    print_opt_src("Taupunkt:", &v.dew_point, "°C", 1, "T");
                    print_opt_src("Feuchtigkeit:", &v.humidity, "%", 0, "T");
                    print_opt_src("Wind (10min):", &v.wind_speed_avg_10min, "m/s", 1, "T");
                    print_opt_src("Böen (max):", &v.wind_gust_max_10min, "m/s", 1, "T");
                    print_opt_src("Windstärke:", &v.wind_force_avg_10min, "bft", 0, "T");
                    if let Some(x) = &v.wind_direction {
                        if let Some(deg) = x.value {
                            println!("  {:<20} {:>6.0}° ({})  (T)", "Windrichtung:", deg, wind_direction_label(deg));
                        }
                    }
                    print_opt_src("Luftdruck:", &v.barometric_pressure_qfe, "hPa", 0, "T");
                }

                if let Some(mq) = resp_mq.result.last() {
                    let v = &mq.values;
                    print_opt_src("Niederschlag:", &v.precipitation, "mm", 1, "M");
                    print_opt_src("Strahlung:", &v.global_radiation, "W/m²", 0, "M");
                    print_opt_src("Pegel:", &v.water_level, "m", 2, "M");
                    println!("  {:<20} {:>6.1} °C  (M)", "Wassertemp (M):", v.water_temperature.value.unwrap_or(f64::NAN));
                }

                if resp_tb.result.is_empty() && resp_mq.result.is_empty() {
                    println!("  Keine aktuellen Daten verfügbar.");
                }
                println!();
            } else {
                // Historical: paginate through data, sample daily
                let start_date = start.unwrap_or_else(|| {
                    (now - chrono::Duration::days(90))
                        .format("%Y-%m-%d")
                        .to_string()
                });

                println!(
                    "\n Zürichsee Wassertemperatur — {}",
                    station
                );
                println!(
                    "  Zeitraum: {} bis {}",
                    start_date, end_date
                );
                println!("  Quelle: Stadt Zürich / Wasserschutzpolizei\n");

                // Fetch with high limit (API max 1000 per call)
                // For 3 months at 10-min intervals = ~13000 points
                // We paginate and sample one value per day (noon)
                let mut all_points: Vec<(String, f64, f64)> = Vec::new(); // (date, water_t, air_t)
                let mut offset = 0u32;
                let page_size = 1000u32;
                let mut last_date = String::new();

                loop {
                    let url = format!(
                        "{}/measurements/{}?startDate={}&endDate={}&sort=timestamp_cet%20asc&limit={}&offset={}",
                        TECDOTTIR_URL, station, start_date, end_date, page_size, offset
                    );
                    let resp: TecdottirResponse = client.get(&url).send().await?.json().await?;
                    let count = resp.result.len();

                    for m in &resp.result {
                        // Take one reading per day (the first one we see for each date)
                        let date = m.timestamp[..10].to_string();
                        if date != last_date {
                            let water_t = m.values.water_temperature.value.unwrap_or(f64::NAN);
                            let air_t = m.values.air_temperature.value.unwrap_or(f64::NAN);
                            if !water_t.is_nan() {
                                all_points.push((date.clone(), water_t, air_t));
                            }
                            last_date = date;
                        }
                    }

                    if count < page_size as usize {
                        break;
                    }
                    offset += page_size;

                    // Safety: don't fetch more than ~15000 records
                    if offset > 15000 {
                        break;
                    }
                }

                if all_points.is_empty() {
                    println!("  Keine Daten gefunden.");
                    return Ok(());
                }

                println!(
                    "  {:<12} {:>10} {:>10}",
                    "Datum", "Wasser °C", "Luft °C"
                );
                println!("  {}", "-".repeat(35));

                let mut min_w = f64::INFINITY;
                let mut max_w = f64::NEG_INFINITY;
                let mut sum_w = 0.0;

                for (date, water_t, air_t) in &all_points {
                    // Reformat date from YYYY-MM-DD to DD.MM.YYYY
                    let nice_date = if date.len() == 10 {
                        format!("{}.{}.{}", &date[8..10], &date[5..7], &date[0..4])
                    } else {
                        date.clone()
                    };
                    println!(
                        "  {:<12} {:>10.1} {:>10.1}",
                        nice_date, water_t, air_t
                    );
                    if *water_t < min_w {
                        min_w = *water_t;
                    }
                    if *water_t > max_w {
                        max_w = *water_t;
                    }
                    sum_w += water_t;
                }

                let avg_w = sum_w / all_points.len() as f64;
                println!("\n  {}", "-".repeat(35));
                println!("  {:<12} {:>10.1} °C", "Minimum", min_w);
                println!("  {:<12} {:>10.1} °C", "Maximum", max_w);
                println!("  {:<12} {:>10.1} °C", "Durchschnitt", avg_w);
                println!("  Datenpunkte: {} Tage", all_points.len());
                println!();
            }
        }

        Commands::Silvaplana { start, end, datum, aktuell } => {
            let now = Utc::now();
            let today = now.format("%Y-%m-%d").to_string();

            if aktuell {
                // Current values: SMN (SIA) + Hydro (2073)
                let (smn, hydro) = tokio::try_join!(
                    fetch_smn_latest(&client, "SIA"),
                    fetch_latest(&client, "2073", "height"),
                )?;

                println!("\n Silvaplana — aktuelle Messwerte");
                println!("  Quellen: MeteoSwiss SIA (Segl-Maria) + BAFU 2073\n");

                if let Some(m) = smn.first() {
                    println!("  Zeitpunkt:        {}", format_timestamp(m.timestamp));
                }
                println!("  {}", "-".repeat(45));

                // Show SMN data in a nice order
                let order = ["dd", "ff", "fx", "tt", "td", "rh", "qfe", "rr", "ss", "rad"];
                for par in &order {
                    if let Some(m) = smn.iter().find(|m| m.par == *par) {
                        if *par == "dd" {
                            println!(
                                "  {:<20} {:>6.0}{}  ({})  (SIA)",
                                "Windrichtung:", m.val, smn_unit(par), wind_direction_label(m.val)
                            );
                        } else {
                            let decimals = match *par {
                                "dd" | "ss" | "rad" => 0,
                                _ => 1,
                            };
                            match decimals {
                                0 => println!("  {:<20} {:>6.0} {}  (SIA)", smn_label(par), m.val, smn_unit(par)),
                                _ => println!("  {:<20} {:>6.1} {}  (SIA)", smn_label(par), m.val, smn_unit(par)),
                            }
                        }
                    }
                }

                // Pegel from BAFU
                if let Some(m) = hydro.first() {
                    println!("  {:<20} {:>6.2} {}  (BAFU 2073)", "Pegel:", m.val, "m ü.M.");
                }
                println!();

            } else if let Some(ref tag) = datum {
                // All 10-minute values for a single day
                let next_day = chrono::NaiveDate::parse_from_str(tag, "%Y-%m-%d")
                    .map(|d| d.succ_opt().unwrap_or(d).format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|_| tag.clone());

                let smn = fetch_smn_daterange(&client, "SIA", tag, &next_day).await?;

                println!("\n Silvaplana — alle Messwerte {}", tag);
                println!("  Quelle: MeteoSwiss SIA (Segl-Maria)\n");

                if smn.is_empty() {
                    println!("  Keine Daten für diesen Tag.");
                    return Ok(());
                }

                // Group by timestamp
                let mut by_ts: std::collections::BTreeMap<i64, HashMap<String, f64>> = std::collections::BTreeMap::new();
                for m in &smn {
                    by_ts.entry(m.timestamp).or_default().insert(m.par.clone(), m.val);
                }

                println!(
                    "  {:<6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5}",
                    "Zeit", "Wind", "Böen", "Ri°", "Temp", "Taup", "Feu%", "Druck", "Regen", "Sonne", "Strhl"
                );
                println!(
                    "  {:<6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5}",
                    "", "km/h", "km/h", "", "°C", "°C", "%", "hPa", "mm", "min", "W/m²"
                );
                println!("  {}", "-".repeat(75));

                let mut max_ff = f64::NEG_INFINITY;
                let mut max_fx = f64::NEG_INFINITY;
                let mut max_ff_time = String::new();
                let mut max_fx_time = String::new();

                for (ts, vals) in &by_ts {
                    let time = format_timestamp(*ts);
                    let short_time = if time.len() >= 16 { &time[11..16] } else { &time };

                    let ff = vals.get("ff").copied().unwrap_or(f64::NAN);
                    let fx = vals.get("fx").copied().unwrap_or(f64::NAN);
                    let dd = vals.get("dd").copied().unwrap_or(f64::NAN);
                    let tt = vals.get("tt").copied().unwrap_or(f64::NAN);
                    let td = vals.get("td").copied().unwrap_or(f64::NAN);
                    let rh = vals.get("rh").copied().unwrap_or(f64::NAN);
                    let qfe = vals.get("qfe").copied().unwrap_or(f64::NAN);
                    let rr = vals.get("rr").copied().unwrap_or(f64::NAN);
                    let ss = vals.get("ss").copied().unwrap_or(f64::NAN);
                    let rad = vals.get("rad").copied().unwrap_or(f64::NAN);

                    let dir_str = if dd.is_nan() { "-".into() } else { format!("{:.0} {}", dd, wind_direction_label(dd)) };

                    println!(
                        "  {:<6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5}",
                        short_time,
                        fmt_opt_f1(Some(ff)), fmt_opt_f1(Some(fx)), dir_str,
                        fmt_opt_f1(Some(tt)), fmt_opt_f1(Some(td)), fmt_opt_f0(Some(rh)),
                        fmt_opt_f1(Some(qfe)), fmt_opt_f1(Some(rr)),
                        fmt_opt_f0(Some(ss)), fmt_opt_f0(Some(rad)),
                    );

                    if !ff.is_nan() && ff > max_ff { max_ff = ff; max_ff_time = short_time.to_string(); }
                    if !fx.is_nan() && fx > max_fx { max_fx = fx; max_fx_time = short_time.to_string(); }
                }

                println!("\n  {}", "-".repeat(75));
                if max_ff > f64::NEG_INFINITY {
                    println!("  Wind max: {:.1} km/h ({}), Böen max: {:.1} km/h ({})",
                        max_ff, max_ff_time, max_fx, max_fx_time);
                }
                println!("  Messpunkte: {}", by_ts.len());
                println!();

            } else {
                // Date range: daily summary
                let start_date = start.unwrap_or_else(|| {
                    (now - chrono::Duration::days(30)).format("%Y-%m-%d").to_string()
                });
                let end_date = end.unwrap_or(today);

                let (smn, hydro_points) = tokio::try_join!(
                    fetch_smn_daterange(&client, "SIA", &start_date, &end_date),
                    fetch_history(&client, "2073", "height", "30d", "1d"),
                )?;

                println!("\n Silvaplana — Tagesübersicht");
                println!("  Zeitraum: {} bis {}", start_date, end_date);
                println!("  Quellen: MeteoSwiss SIA + BAFU 2073\n");

                // Group SMN by day, compute daily stats
                let mut by_day: std::collections::BTreeMap<String, Vec<&Measurement>> = std::collections::BTreeMap::new();
                for m in &smn {
                    let day = format_timestamp(m.timestamp);
                    let day_str = if day.len() >= 10 { day[..10].to_string() } else { day };
                    by_day.entry(day_str).or_default().push(m);
                }

                // Index hydro by date
                let pegel_by_day: HashMap<String, f64> = hydro_points.iter()
                    .map(|p| (p.time[..10].to_string(), p.value))
                    .collect();

                println!(
                    "  {:<12} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8}",
                    "Datum", "Wind", "Böen", "Ri°", "Temp", "Regen", "Pegel"
                );
                println!(
                    "  {:<12} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8}",
                    "", "km/h", "km/h", "avg", "°C", "mm", "m ü.M."
                );
                println!("  {}", "-".repeat(55));

                for (day, measures) in &by_day {
                    let avg = |par: &str| -> f64 {
                        let vals: Vec<f64> = measures.iter()
                            .filter(|m| m.par == par)
                            .map(|m| m.val)
                            .collect();
                        if vals.is_empty() { f64::NAN } else { vals.iter().sum::<f64>() / vals.len() as f64 }
                    };
                    let max = |par: &str| -> f64 {
                        measures.iter()
                            .filter(|m| m.par == par)
                            .map(|m| m.val)
                            .fold(f64::NEG_INFINITY, f64::max)
                    };
                    let sum = |par: &str| -> f64 {
                        measures.iter()
                            .filter(|m| m.par == par)
                            .map(|m| m.val)
                            .sum()
                    };

                    let ff_max = max("ff");
                    let fx_max = max("fx");
                    let dd_avg = avg("dd");
                    let tt_avg = avg("tt");
                    let rr_sum = sum("rr");
                    let pegel = pegel_by_day.get(day).copied().unwrap_or(f64::NAN);

                    let dir_str = if dd_avg.is_nan() { "-".into() } else {
                        wind_direction_label(dd_avg).to_string()
                    };

                    println!(
                        "  {:<12} {:>6} {:>6} {:>6} {:>6} {:>6} {:>8}",
                        day,
                        fmt_opt_f1(Some(ff_max)),
                        fmt_opt_f1(Some(fx_max)),
                        dir_str,
                        fmt_opt_f1(Some(tt_avg)),
                        fmt_opt_f1(Some(rr_sum)),
                        if pegel.is_nan() { "-".into() } else { format!("{:.2}", pegel) },
                    );
                }
                println!();
            }
        }

        Commands::Sihlsee { start, end, datum, aktuell } => {
            let now = Utc::now();
            let today = now.format("%Y-%m-%d").to_string();

            if aktuell {
                let smn = fetch_smn_latest(&client, "EIN").await?;

                println!("\n Sihlsee — aktuelle Messwerte");
                println!("  Quelle: MeteoSwiss EIN (Einsiedeln, 1.8 km vom Sihlsee)");
                println!("  Station: https://maps.google.com/?q=47.1330,8.7566\n");

                if let Some(m) = smn.first() {
                    println!("  Zeitpunkt:        {}", format_timestamp(m.timestamp));
                }
                println!("  {}", "-".repeat(45));

                let order = ["dd", "ff", "fx", "tt", "td", "rh", "qfe", "rr", "ss", "rad"];
                for par in &order {
                    if let Some(m) = smn.iter().find(|m| m.par == *par) {
                        if *par == "dd" {
                            println!("  {:<20} {:>6.0}° ({})  (EIN)", "Windrichtung:", m.val, wind_direction_label(m.val));
                        } else {
                            let decimals = match *par { "dd" | "ss" | "rad" => 0, _ => 1 };
                            match decimals {
                                0 => println!("  {:<20} {:>6.0} {}  (EIN)", smn_label(par), m.val, smn_unit(par)),
                                _ => println!("  {:<20} {:>6.1} {}  (EIN)", smn_label(par), m.val, smn_unit(par)),
                            }
                        }
                    }
                }
                println!();

            } else if let Some(ref tag) = datum {
                let next_day = chrono::NaiveDate::parse_from_str(tag, "%Y-%m-%d")
                    .map(|d| d.succ_opt().unwrap_or(d).format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|_| tag.clone());

                let smn = fetch_smn_daterange(&client, "EIN", tag, &next_day).await?;

                println!("\n Sihlsee — alle Messwerte {}", tag);
                println!("  Quelle: MeteoSwiss EIN (Einsiedeln)");
                println!("  Station: https://maps.google.com/?q=47.1330,8.7566\n");

                if smn.is_empty() {
                    println!("  Keine Daten für diesen Tag.");
                    return Ok(());
                }

                let mut by_ts: std::collections::BTreeMap<i64, HashMap<String, f64>> = std::collections::BTreeMap::new();
                for m in &smn { by_ts.entry(m.timestamp).or_default().insert(m.par.clone(), m.val); }

                println!(
                    "  {:<6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5}",
                    "Zeit", "Wind", "Böen", "Ri°", "Temp", "Taup", "Feu%", "Druck", "Regen", "Sonne", "Strhl"
                );
                println!(
                    "  {:<6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5}",
                    "", "km/h", "km/h", "", "°C", "°C", "%", "hPa", "mm", "min", "W/m²"
                );
                println!("  {}", "-".repeat(75));

                let mut max_ff = f64::NEG_INFINITY;
                let mut max_fx = f64::NEG_INFINITY;
                let mut max_ff_time = String::new();
                let mut max_fx_time = String::new();

                for (ts, vals) in &by_ts {
                    let time = format_timestamp(*ts);
                    let short_time = if time.len() >= 16 { &time[11..16] } else { &time };

                    let ff = vals.get("ff").copied().unwrap_or(f64::NAN);
                    let fx = vals.get("fx").copied().unwrap_or(f64::NAN);
                    let dd = vals.get("dd").copied().unwrap_or(f64::NAN);
                    let tt = vals.get("tt").copied().unwrap_or(f64::NAN);
                    let td = vals.get("td").copied().unwrap_or(f64::NAN);
                    let rh = vals.get("rh").copied().unwrap_or(f64::NAN);
                    let qfe = vals.get("qfe").copied().unwrap_or(f64::NAN);
                    let rr = vals.get("rr").copied().unwrap_or(f64::NAN);
                    let ss = vals.get("ss").copied().unwrap_or(f64::NAN);
                    let rad = vals.get("rad").copied().unwrap_or(f64::NAN);

                    let dir_str = if dd.is_nan() { "-".into() } else { format!("{:.0} {}", dd, wind_direction_label(dd)) };

                    println!(
                        "  {:<6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>6} {:>5} {:>5} {:>5}",
                        short_time, fmt_opt_f1(Some(ff)), fmt_opt_f1(Some(fx)), dir_str,
                        fmt_opt_f1(Some(tt)), fmt_opt_f1(Some(td)), fmt_opt_f0(Some(rh)),
                        fmt_opt_f1(Some(qfe)), fmt_opt_f1(Some(rr)), fmt_opt_f0(Some(ss)), fmt_opt_f0(Some(rad)),
                    );

                    if !ff.is_nan() && ff > max_ff { max_ff = ff; max_ff_time = short_time.to_string(); }
                    if !fx.is_nan() && fx > max_fx { max_fx = fx; max_fx_time = short_time.to_string(); }
                }

                println!("\n  {}", "-".repeat(75));
                if max_ff > f64::NEG_INFINITY {
                    println!("  Wind max: {:.1} km/h ({}), Böen max: {:.1} km/h ({})", max_ff, max_ff_time, max_fx, max_fx_time);
                }
                println!("  Messpunkte: {}", by_ts.len());
                println!();

            } else {
                let start_date = start.unwrap_or_else(|| {
                    (now - chrono::Duration::days(30)).format("%Y-%m-%d").to_string()
                });
                let end_date = end.unwrap_or(today);

                let smn = fetch_smn_daterange(&client, "EIN", &start_date, &end_date).await?;

                println!("\n Sihlsee — Tagesübersicht");
                println!("  Zeitraum: {} bis {}", start_date, end_date);
                println!("  Quelle: MeteoSwiss EIN (Einsiedeln)");
                println!("  Station: https://maps.google.com/?q=47.1330,8.7566\n");

                let mut by_day: std::collections::BTreeMap<String, Vec<&Measurement>> = std::collections::BTreeMap::new();
                for m in &smn {
                    let day = format_timestamp(m.timestamp);
                    let day_str = if day.len() >= 10 { day[..10].to_string() } else { day };
                    by_day.entry(day_str).or_default().push(m);
                }

                println!(
                    "  {:<12} {:>6} {:>6} {:>6} {:>6} {:>6}",
                    "Datum", "Wind", "Böen", "Ri°", "Temp", "Regen"
                );
                println!(
                    "  {:<12} {:>6} {:>6} {:>6} {:>6} {:>6}",
                    "", "km/h", "km/h", "avg", "°C", "mm"
                );
                println!("  {}", "-".repeat(48));

                for (day, measures) in &by_day {
                    let avg = |par: &str| -> f64 {
                        let vals: Vec<f64> = measures.iter().filter(|m| m.par == par).map(|m| m.val).collect();
                        if vals.is_empty() { f64::NAN } else { vals.iter().sum::<f64>() / vals.len() as f64 }
                    };
                    let max = |par: &str| -> f64 {
                        measures.iter().filter(|m| m.par == par).map(|m| m.val).fold(f64::NEG_INFINITY, f64::max)
                    };
                    let sum = |par: &str| -> f64 {
                        measures.iter().filter(|m| m.par == par).map(|m| m.val).sum()
                    };

                    let ff_max = max("ff");
                    let fx_max = max("fx");
                    let dd_avg = avg("dd");
                    let tt_avg = avg("tt");
                    let rr_sum = sum("rr");

                    let dir_str = if dd_avg.is_nan() { "-".into() } else { wind_direction_label(dd_avg).to_string() };

                    println!(
                        "  {:<12} {:>6} {:>6} {:>6} {:>6} {:>6}",
                        day, fmt_opt_f1(Some(ff_max)), fmt_opt_f1(Some(fx_max)),
                        dir_str, fmt_opt_f1(Some(tt_avg)), fmt_opt_f1(Some(rr_sum)),
                    );
                }
                println!();
            }
        }

        Commands::Ermioni { start, end, aktuell } => {
            let now = Utc::now();
            let today = now.format("%Y-%m-%d").to_string();

            if aktuell {
                let meteo = fetch_open_meteo_current(&client).await?;

                println!("\n Ermioni — aktuelle Messwerte");
                println!("  Quelle: Open-Meteo (37.38°N, 23.25°E)\n");

                if let Some(c) = &meteo.current {
                    println!("  {}", "-".repeat(40));
                    if let Some(v) = c.wind_speed_10m { println!("  {:<20} {:>6.1} km/h", "Wind:", v); }
                    if let Some(v) = c.wind_gusts_10m { println!("  {:<20} {:>6.1} km/h", "Böen:", v); }
                    if let Some(v) = c.wind_direction_10m {
                        println!("  {:<20} {:>6.0}° ({})", "Windrichtung:", v, wind_direction_label(v));
                    }
                    if let Some(v) = c.temperature_2m { println!("  {:<20} {:>6.1} °C", "Temperatur:", v); }
                }

                // Try Poseidon (username/password or client_id/secret)
                let poseidon_creds = std::env::var("POSEIDON_USER")
                    .and_then(|u| std::env::var("POSEIDON_PASS").map(|p| (u, p)))
                    .or_else(|_| {
                        std::env::var("POSEIDON_CLIENT_ID")
                            .and_then(|u| std::env::var("POSEIDON_CLIENT_SECRET").map(|p| (u, p)))
                    });

                if let Ok((user, pass)) = poseidon_creds {
                    match fetch_poseidon_token(&client, &user, &pass).await {
                        Ok(token) => {
                            println!("\n  Poseidon — Login OK!");
                            match fetch_poseidon_platforms(&client, &token).await {
                                Ok(platforms) => {
                                    println!("  Plattformen: {}", platforms);
                                }
                                Err(e) => println!("  Plattformen-Fehler: {}", e),
                            }
                        }
                        Err(e) => println!("\n  Poseidon Token-Fehler: {}", e),
                    }
                } else {
                    println!("\n  Poseidon: POSEIDON_USER / POSEIDON_PASS nicht gesetzt.");
                    println!("  Registrieren: https://auth.poseidon.hcmr.gr/auth/register/");
                }
                println!();

            } else {
                // Date range with hourly data
                let start_date = start.unwrap_or_else(|| {
                    (now - chrono::Duration::days(7)).format("%Y-%m-%d").to_string()
                });
                let end_date = end.unwrap_or(today);

                // Determine if archive API needed
                let is_archive = start_date < (now - chrono::Duration::days(2)).format("%Y-%m-%d").to_string();

                println!("\n Ermioni — Stündliche Werte");
                println!("  Zeitraum: {} bis {}", start_date, end_date);
                println!("  Quelle: Open-Meteo\n");

                let (meteo, marine) = tokio::try_join!(
                    fetch_open_meteo_hourly(&client, &start_date, &end_date, is_archive),
                    fetch_open_meteo_marine(&client, &start_date, &end_date),
                )?;

                if let Some(h) = &meteo.hourly {
                    let ff = h.wind_speed_10m.as_ref();
                    let fx = h.wind_gusts_10m.as_ref();
                    let dd = h.wind_direction_10m.as_ref();
                    let tt = h.temperature_2m.as_ref();
                    let waves = marine.hourly.as_ref();
                    let wh = waves.and_then(|w| w.wave_height.as_ref());

                    println!(
                        "  {:<18} {:>6} {:>6} {:>6} {:>6} {:>6}",
                        "Zeit", "Wind", "Böen", "Ri°", "Temp", "Welle"
                    );
                    println!(
                        "  {:<18} {:>6} {:>6} {:>6} {:>6} {:>6}",
                        "", "km/h", "km/h", "", "°C", "m"
                    );
                    println!("  {}", "-".repeat(55));

                    let mut max_ff = f64::NEG_INFINITY;
                    let mut max_fx = f64::NEG_INFINITY;

                    for (i, time) in h.time.iter().enumerate() {
                        let w = ff.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                        let g = fx.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                        let d = dd.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                        let t = tt.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                        let wave = wh.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);

                        let dir_str = if d.is_nan() { "-".into() } else {
                            format!("{:.0} {}", d, wind_direction_label(d))
                        };

                        // Short time label
                        let label = if time.len() >= 13 { &time[5..13] } else { time };

                        println!(
                            "  {:<18} {:>6} {:>6} {:>6} {:>6} {:>6}",
                            label,
                            fmt_opt_f1(Some(w)), fmt_opt_f1(Some(g)), dir_str,
                            fmt_opt_f1(Some(t)), fmt_opt_f1(Some(wave)),
                        );

                        if !w.is_nan() && w > max_ff { max_ff = w; }
                        if !g.is_nan() && g > max_fx { max_fx = g; }
                    }

                    println!("\n  {}", "-".repeat(55));
                    if max_ff > f64::NEG_INFINITY {
                        println!("  Wind max: {:.1} km/h, Böen max: {:.1} km/h", max_ff, max_fx);
                    }
                    println!("  Datenpunkte: {}", h.time.len());
                } else {
                    println!("  Keine Daten gefunden.");
                }
                println!();
            }
        }

        Commands::Report { start, end, output, svg, silvaplana: silv, neuenburgersee: neuen, urnersee: urner, greifensee: greif, sihlsee: sihl, ermioni: erm } => {

            // --- Ermioni report (Open-Meteo based) ---
            if erm {
                println!("  Lade Daten Open-Meteo + Marine ({} bis {})...", start, end);

                let is_archive = start < (Utc::now() - chrono::Duration::days(2)).format("%Y-%m-%d").to_string();

                let (meteo, marine) = tokio::try_join!(
                    fetch_open_meteo_hourly(&client, &start, &end, is_archive),
                    fetch_open_meteo_marine(&client, &start, &end),
                )?;

                let h = meteo.hourly.ok_or("Keine Wetterdaten")?;
                let mh = marine.hourly;

                // Build JSON rows: [label, ff, fx, dd, tt, rh, pressure, wave_h, wave_dir, wave_period]
                let mut json_rows: Vec<String> = Vec::new();
                let mut max_ff = f64::NEG_INFINITY;
                let mut max_ff_time = String::new();
                let mut max_fx = f64::NEG_INFINITY;
                let mut max_fx_time = String::new();
                let mut min_tt = f64::INFINITY;
                let mut min_tt_time = String::new();
                let mut max_tt = f64::NEG_INFINITY;
                let mut max_tt_time = String::new();
                let mut max_wh = f64::NEG_INFINITY;
                let mut max_wh_time = String::new();

                let ff_v = h.wind_speed_10m.as_ref();
                let fx_v = h.wind_gusts_10m.as_ref();
                let dd_v = h.wind_direction_10m.as_ref();
                let tt_v = h.temperature_2m.as_ref();
                let rh_v = h.relative_humidity_2m.as_ref();
                let pr_v = h.pressure_msl.as_ref();
                let wh_v = mh.as_ref().and_then(|m| m.wave_height.as_ref());
                let wd_v = mh.as_ref().and_then(|m| m.wind_wave_direction.as_ref());
                let wp_v = mh.as_ref().and_then(|m| m.wind_wave_period.as_ref());

                for (i, time) in h.time.iter().enumerate() {
                    let label = if time.len() >= 13 {
                        let d = &time[8..10];
                        let m = &time[5..7];
                        let t = &time[11..13];
                        format!("{}.{}. {}:00", d.trim_start_matches('0'), m.trim_start_matches('0'), t)
                    } else {
                        time.clone()
                    };

                    let ff = ff_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let fx = fx_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let dd = dd_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let tt = tt_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let rh = rh_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let pr = pr_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let wh = wh_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let wdir = wd_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);
                    let wp = wp_v.and_then(|v| v.get(i).copied().flatten()).unwrap_or(f64::NAN);

                    fn jv(v: f64) -> String { if v.is_nan() { "null".into() } else { format!("{}", v) } }

                    json_rows.push(format!(
                        "[\"{}\",{},{},{},{},{},{},{},{},{}]",
                        label, jv(ff), jv(fx), jv(dd), jv(tt), jv(rh), jv(pr), jv(wh), jv(wdir), jv(wp)
                    ));

                    if !ff.is_nan() && ff > max_ff { max_ff = ff; max_ff_time = label.clone(); }
                    if !fx.is_nan() && fx > max_fx { max_fx = fx; max_fx_time = label.clone(); }
                    if !tt.is_nan() && tt < min_tt { min_tt = tt; min_tt_time = label.clone(); }
                    if !tt.is_nan() && tt > max_tt { max_tt = tt; max_tt_time = label.clone(); }
                    if !wh.is_nan() && wh > max_wh { max_wh = wh; max_wh_time = label.clone(); }
                }

                if json_rows.is_empty() {
                    println!("  Keine Daten gefunden.");
                    return Ok(());
                }

                let output_path = output.unwrap_or_else(|| {
                    format!("html/ermioni_{}_{}.html", start, end)
                });
                if let Some(parent) = std::path::Path::new(&output_path).parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let chartjs = include_str!("chartjs.min.js");
                let mut f = std::fs::File::create(&output_path)?;

                write!(f, r#"<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Ermioni — {start} bis {end}</title>
<script>
{chartjs}
</script>
<style>
  :root {{ --bg: #f8f9fa; --card: #fff; --text: #212529; --muted: #6c757d; --border: #dee2e6; --blue: #0d6efd; --red: #dc3545; --green: #198754; --cyan: #0dcaf0; --orange: #fd7e14; --purple: #6f42c1; }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: var(--bg); color: var(--text); line-height: 1.6; padding: 1rem; }}
  .container {{ max-width: 1400px; margin: 0 auto; }}
  h1 {{ font-size: 1.5rem; margin-bottom: 0.25rem; }}
  .subtitle {{ color: var(--muted); margin-bottom: 0.5rem; font-size: 0.9rem; }}
  .sources {{ color: var(--muted); margin-bottom: 1rem; font-size: 0.8rem; background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 0.5rem 1rem; }}
  .sources strong {{ color: var(--text); }}
  .sources a {{ color: var(--blue); }}
  .stats {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 0.75rem; margin-bottom: 1.5rem; }}
  .stat {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 0.75rem 1rem; }}
  .stat .label {{ font-size: 0.7rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; }}
  .stat .value {{ font-size: 1.4rem; font-weight: 700; }}
  .stat .unit {{ font-size: 0.8rem; color: var(--muted); }}
  .chart-card {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 1rem; margin-bottom: 1rem; }}
  .chart-card h2 {{ font-size: 1rem; margin-bottom: 0.75rem; }}
  .chart-card .src-note {{ font-size: 0.7rem; color: var(--muted); margin-bottom: 0.5rem; }}
  .chart-wrap {{ position: relative; height: 300px; }}
  .charts-grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }}
  @media (max-width: 900px) {{ .charts-grid {{ grid-template-columns: 1fr; }} }}
  table {{ width: 100%; border-collapse: collapse; font-size: 0.75rem; font-variant-numeric: tabular-nums; }}
  th, td {{ padding: 3px 6px; text-align: right; border-bottom: 1px solid var(--border); white-space: nowrap; }}
  th {{ background: var(--bg); position: sticky; top: 0; font-weight: 600; z-index: 1; }}
  td:first-child, th:first-child {{ text-align: left; }}
  .table-wrap {{ max-height: 600px; overflow: auto; border: 1px solid var(--border); border-radius: 8px; }}
  .day-header {{ background: #e9ecef; font-weight: 700; }}
  footer {{ margin-top: 2rem; padding-top: 1rem; border-top: 1px solid var(--border); font-size: 0.75rem; color: var(--muted); }}
</style>
</head>
<body>
<div class="container">

<h1>Ermioni — Argolischer Golf</h1>
<p class="subtitle">{start} bis {end}</p>
<div class="sources">
  <strong>Open-Meteo:</strong> Wind, Temperatur, Feuchtigkeit, Luftdruck (Modell-Daten für 37.38°N, 23.25°E)<br>
  <strong>Open-Meteo Marine:</strong> Wellenhöhe, Wellenrichtung, Wellenperiode
</div>
<div class="sources">
  <strong>Messstationen:</strong><br>
  <a href="https://maps.google.com/?q=37.38,23.25" target="_blank" rel="noopener">Ermioni — Modell-Gitterpunkt (Open-Meteo)</a><br>
  <a href="https://maps.google.com/?q=37.611,23.564" target="_blank" rel="noopener">Saronikos-Boje (Poseidon/HCMR) — ~30 km NE</a>
</div>

<div class="sources">
  <strong>Webcams:</strong><br>
  <a href="https://www.webcamgreece.com/webcam-hydra-live.html" target="_blank" rel="noopener">Hydra Live (Argolischer Golf)</a><br>
  <a href="https://city-webcams.com/live?streaming=greece-v&amp;webcam=Ermioni" target="_blank" rel="noopener">Ermioni Webcam</a>
</div>

<div class="stats">
  <div class="stat"><div class="label">Wind Max</div><div class="value" style="color:var(--green)">{max_ff:.1} <span class="unit">km/h</span></div><div class="label">{max_ff_time}</div></div>
  <div class="stat"><div class="label">Böen Max</div><div class="value" style="color:var(--orange)">{max_fx:.1} <span class="unit">km/h</span></div><div class="label">{max_fx_time}</div></div>
  <div class="stat"><div class="label">Temp Min</div><div class="value" style="color:var(--blue)">{min_tt:.1} <span class="unit">&deg;C</span></div><div class="label">{min_tt_time}</div></div>
  <div class="stat"><div class="label">Temp Max</div><div class="value" style="color:var(--red)">{max_tt:.1} <span class="unit">&deg;C</span></div><div class="label">{max_tt_time}</div></div>
  <div class="stat"><div class="label">Welle Max</div><div class="value" style="color:var(--cyan)">{max_wh:.1} <span class="unit">m</span></div><div class="label">{max_wh_time}</div></div>
</div>

<div class="chart-card">
  <h2>Wind &amp; Böen</h2>
  <div class="chart-wrap"><canvas id="chartWind"></canvas></div>
</div>

<div class="charts-grid">
  <div class="chart-card"><h2>Windrichtung</h2><div class="chart-wrap"><canvas id="chartWindDir"></canvas></div></div>
  <div class="chart-card"><h2>Temperatur</h2><div class="chart-wrap"><canvas id="chartTemp"></canvas></div></div>
</div>

<div class="charts-grid">
  <div class="chart-card"><h2>Wellenhöhe</h2><div class="chart-wrap"><canvas id="chartWave"></canvas></div></div>
  <div class="chart-card"><h2>Luftdruck &amp; Feuchtigkeit</h2><div class="chart-wrap"><canvas id="chartPressure"></canvas></div></div>
</div>

<div class="chart-card">
  <h2>Alle Messwerte — {count} Datenpunkte</h2>
  <div class="table-wrap">
    <table><thead><tr>
      <th>Zeit</th><th>Wind km/h</th><th>Böen km/h</th><th>Richtung</th><th>Temp &deg;C</th>
      <th>Feuchte %</th><th>Druck hPa</th><th>Welle m</th><th>Wellen-Ri</th><th>Periode s</th>
    </tr></thead><tbody id="tableBody"></tbody></table>
  </div>
</div>

<footer>Open-Meteo (open-meteo.com) + Poseidon/HCMR (Saronikos-Boje) &middot; Generiert mit <strong>pegelstand</strong> CLI v{version}</footer>
</div>

<script>
const data = [
{json_data}
];
const labels = data.map(d => d[0]);
const wind = data.map(d => d[1]);
const gusts = data.map(d => d[2]);
const windDir = data.map(d => d[3]);
const temp = data.map(d => d[4]);
const humidity = data.map(d => d[5]);
const pressure = data.map(d => d[6]);
const waveH = data.map(d => d[7]);
const waveDir = data.map(d => d[8]);
const wavePer = data.map(d => d[9]);

function dirLabel(deg) {{
  if (deg === null) return '-';
  const dirs = ['N','NO','O','SO','S','SW','W','NW'];
  return dirs[Math.round(((deg % 360) + 360) % 360 / 45) % 8];
}}

const co = {{
  responsive: true, maintainAspectRatio: false,
  interaction: {{ mode: 'index', intersect: false }},
  plugins: {{ legend: {{ position: 'top', labels: {{ usePointStyle: true, boxWidth: 8, font: {{ size: 11 }} }} }} }},
  scales: {{ x: {{ ticks: {{ maxTicksLimit: 20, maxRotation: 45, font: {{ size: 10 }} }} }} }},
  elements: {{ point: {{ radius: 0 }}, line: {{ borderWidth: 1.5 }} }},
}};

new Chart(document.getElementById('chartWind'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Wind', data: wind, borderColor: '#198754', fill: false }},
    {{ label: 'Böen', data: gusts, borderColor: '#fd7e14', backgroundColor: 'rgba(253,126,20,0.1)', fill: true }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: 'km/h' }} }} }} }}
}});

new Chart(document.getElementById('chartWindDir'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Windrichtung', data: windDir, borderColor: '#6f42c1', fill: false, pointRadius: 2, pointBackgroundColor: '#6f42c1', showLine: false }},
  ] }}, options: {{ ...co, elements: {{ point: {{ radius: 2 }}, line: {{ borderWidth: 0 }} }},
    scales: {{ ...co.scales, y: {{ min: 0, max: 360, title: {{ display: true, text: 'Grad' }},
      ticks: {{ stepSize: 45, callback: function(v) {{ const d = {{0:'N',45:'NO',90:'O',135:'SO',180:'S',225:'SW',270:'W',315:'NW',360:'N'}}; return d[v] || v+'°'; }} }} }} }} }}
}});

new Chart(document.getElementById('chartTemp'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Temperatur', data: temp, borderColor: '#dc3545', fill: false }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: '°C' }} }} }} }}
}});

new Chart(document.getElementById('chartWave'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Wellenhöhe', data: waveH, borderColor: '#0dcaf0', backgroundColor: 'rgba(13,202,240,0.1)', fill: true }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: 'm' }}, min: 0 }} }} }}
}});

new Chart(document.getElementById('chartPressure'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Luftdruck', data: pressure, borderColor: '#6f42c1', yAxisID: 'y', fill: false }},
    {{ label: 'Feuchtigkeit', data: humidity, borderColor: '#0dcaf0', yAxisID: 'y1', fill: false, borderDash: [4,4] }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ position: 'left', title: {{ display: true, text: 'hPa' }} }}, y1: {{ position: 'right', title: {{ display: true, text: '%' }}, grid: {{ drawOnChartArea: false }} }} }} }}
}});

const tbody = document.getElementById('tableBody');
let lastDay = '';
const fmt = (v, d) => v === null ? '-' : d === 0 ? Math.round(v).toString() : v.toFixed(d);
data.forEach(d => {{
  const day = d[0].split(' ')[0];
  if (day !== lastDay) {{
    const tr = document.createElement('tr');
    tr.className = 'day-header';
    tr.innerHTML = '<td colspan="10">' + day + '</td>';
    tbody.appendChild(tr);
    lastDay = day;
  }}
  const tr = document.createElement('tr');
  const dirStr = d[3] !== null ? Math.round(d[3]) + '° ' + dirLabel(d[3]) : '-';
  const wdirStr = d[8] !== null ? Math.round(d[8]) + '° ' + dirLabel(d[8]) : '-';
  tr.innerHTML = '<td>' + d[0].split(' ')[1] + '</td>'
    + '<td>' + fmt(d[1],1) + '</td><td>' + fmt(d[2],1) + '</td><td>' + dirStr + '</td>'
    + '<td>' + fmt(d[4],1) + '</td><td>' + fmt(d[5],0) + '</td><td>' + fmt(d[6],0) + '</td>'
    + '<td>' + fmt(d[7],1) + '</td><td>' + wdirStr + '</td><td>' + fmt(d[9],1) + '</td>';
  tbody.appendChild(tr);
}});
</script>
</body>
</html>"#,
                    start = start, end = end, chartjs = chartjs,
                    max_ff = max_ff, max_ff_time = max_ff_time,
                    max_fx = max_fx, max_fx_time = max_fx_time,
                    min_tt = min_tt, min_tt_time = min_tt_time,
                    max_tt = max_tt, max_tt_time = max_tt_time,
                    max_wh = max_wh, max_wh_time = max_wh_time,
                    count = json_rows.len(), version = APP_VERSION,
                    json_data = json_rows.join(",\n"),
                )?;

                println!("  {} Datenpunkte geschrieben.", json_rows.len());
                println!("  Datei: {}", output_path);
                return Ok(());
            }

            // Determine lake-specific report config
            struct LakeConfig {
                name: &'static str,
                smn_station: &'static str,
                smn_desc: &'static str,
                smn_lat: f64,
                smn_lon: f64,
                bafu_id: &'static str,
                bafu_desc: &'static str,
                bafu_lat: f64,
                bafu_lon: f64,
                webcams: &'static [(&'static str, &'static str)], // (label, url)
            }

            let lake_config: Option<LakeConfig> = if silv {
                Some(LakeConfig {
                    name: "Silvaplana", smn_station: "SIA",
                    smn_desc: "Segl-Maria, 1823 m ü.M., ~3 km vom Silvaplanersee",
                    smn_lat: 46.4323, smn_lon: 9.7623,
                    bafu_id: "2073", bafu_desc: "Silvaplanersee",
                    bafu_lat: 46.4601, bafu_lon: 9.8024,
                    webcams: &[
                        ("Kitespot Webcam", "https://www.kitesailing.ch/en/spot/webcam"),
                        ("Skyline Surfcenter", "https://www.skylinewebcams.com/en/webcam/schweiz/graubunden/silvaplana/silvaplana-surfcenter.html"),
                        ("Roundshot Sils", "https://sils.roundshot.com/"),
                    ],
                })
            } else if neuen {
                Some(LakeConfig {
                    name: "Neuenburgersee", smn_station: "PAY",
                    smn_desc: "Payerne, 491 m ü.M., ~10 km vom Neuenburgersee",
                    smn_lat: 46.8116, smn_lon: 6.9425,
                    bafu_id: "2154", bafu_desc: "Lac de Neuchâtel (Grandson)",
                    bafu_lat: 46.8058, bafu_lon: 6.6424,
                    webcams: &[
                        ("Roundshot Lac de Neuchâtel", "https://lacdeneuchatel.roundshot.com/"),
                        ("Roundshot Neuchâtel", "https://neuchatel.roundshot.com/"),
                    ],
                })
            } else if urner {
                Some(LakeConfig {
                    name: "Urnersee", smn_station: "ALT",
                    smn_desc: "Altdorf, 449 m ü.M., direkt am Urnersee",
                    smn_lat: 46.8871, smn_lon: 8.6219,
                    bafu_id: "2025", bafu_desc: "Vierwaldstättersee (Brunnen)",
                    bafu_lat: 46.9935, bafu_lon: 8.6038,
                    webcams: &[
                        ("Foto-Webcam Brunnen", "https://www.foto-webcam.eu/webcam/brunnen/"),
                        ("Roundshot SGV", "https://sgv.roundshot.com/"),
                        ("Roundshot Morschach", "https://shp.roundshot.com/"),
                    ],
                })
            } else if greif {
                Some(LakeConfig {
                    name: "Greifensee", smn_station: "PFA",
                    smn_desc: "Pfaffikon ZH, 537 m ü.M., ~8 km östlich",
                    smn_lat: 47.3768, smn_lon: 8.7549,
                    bafu_id: "2082", bafu_desc: "Greifensee",
                    bafu_lat: 47.3652, bafu_lon: 8.6735,
                    webcams: &[
                        ("Greifenseewetter.ch", "https://greifenseewetter.ch/webcam2.htm"),
                    ],
                })
            } else if sihl {
                Some(LakeConfig {
                    name: "Sihlsee", smn_station: "EIN",
                    smn_desc: "Einsiedeln, 910 m ü.M., ~1.8 km vom Sihlsee",
                    smn_lat: 47.1330, smn_lon: 8.7566,
                    bafu_id: "2609", bafu_desc: "Alp (Einsiedeln, Zufluss)",
                    bafu_lat: 47.1508, bafu_lon: 8.7393,
                    webcams: &[
                        ("Segelclub Sihlsee", "https://wetter.segelclub-sihlsee.ch/scsws/wetter/webcam.html"),
                        ("Bergfex Willerzell", "https://www.bergfex.ch/sommer/einsiedeln-ybrig-zuerichsee/webcams/c25544/"),
                    ],
                })
            } else {
                None
            };

            if let Some(lc) = lake_config {
            let (lake_name, smn_station, smn_desc, bafu_id, bafu_desc) =
                (lc.name, lc.smn_station, lc.smn_desc, lc.bafu_id, lc.bafu_desc);
                println!("  Lade Daten {} (MeteoSwiss) + BAFU {} ({} bis {})...", smn_station, bafu_id, start, end);

                // Try daterange API first, fall back to InfluxDB for older data
                let smn = fetch_smn_daterange(&client, smn_station, &start, &end).await?;

                let mut by_ts: std::collections::BTreeMap<i64, HashMap<String, f64>> = std::collections::BTreeMap::new();

                if !smn.is_empty() {
                    println!("  {} Messwerte via SMN API.", smn.len());
                    for m in &smn {
                        by_ts.entry(m.timestamp).or_default().insert(m.par.clone(), m.val);
                    }
                } else {
                    // InfluxDB fallback — aggregate hourly for long ranges
                    println!("  SMN API leer, lade via InfluxDB (stündlich aggregiert)...");

                    let _fields = ["ff", "fx", "dd", "tt", "td", "rh", "qfe", "rr", "ss", "rad"];
                    let flux = format!(
                        r#"from(bucket: "existenzApi")
    |> range(start: {start}T00:00:00Z, stop: {end}T23:59:59Z)
    |> filter(fn: (r) => r["_measurement"] == "smn")
    |> filter(fn: (r) => r["loc"] == "{smn_station}")
    |> filter(fn: (r) => r["_field"] == "ff" or r["_field"] == "fx" or r["_field"] == "dd" or r["_field"] == "tt" or r["_field"] == "td" or r["_field"] == "rh" or r["_field"] == "qfe" or r["_field"] == "rr" or r["_field"] == "ss" or r["_field"] == "rad")
    |> aggregateWindow(every: 1h, fn: mean, createEmpty: false)
    |> yield(name: "hourly")"#,
                        start = start, end = end,
                    );

                    let url = format!("{}/api/v2/query?org={}", INFLUX_URL, INFLUX_ORG);
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Token {}", INFLUX_TOKEN))
                        .header("Content-Type", "application/vnd.flux")
                        .header("Accept", "application/csv")
                        .body(flux)
                        .send()
                        .await?;

                    let status = resp.status();
                    let body = resp.text().await?;
                    if !status.is_success() {
                        return Err(format!("InfluxDB Fehler ({}): {}", status, &body[..200.min(body.len())]).into());
                    }

                    let mut rdr = csv::Reader::from_reader(body.as_bytes());
                    let headers = rdr.headers()?.clone();
                    let time_idx = headers.iter().position(|h| h == "_time");
                    let value_idx = headers.iter().position(|h| h == "_value");
                    let field_idx = headers.iter().position(|h| h == "_field");

                    if let (Some(ti), Some(vi), Some(fi)) = (time_idx, value_idx, field_idx) {
                        for result in rdr.records() {
                            let record = result?;
                            if let (Some(time_str), Some(val_str), Some(field)) = (record.get(ti), record.get(vi), record.get(fi)) {
                                if let Ok(val) = val_str.parse::<f64>() {
                                    if let Ok(dt) = time_str.parse::<DateTime<Utc>>() {
                                        let ts = dt.timestamp();
                                        by_ts.entry(ts).or_default().insert(field.to_string(), val);
                                    }
                                }
                            }
                        }
                    }

                    println!("  {} Stunden-Datenpunkte via InfluxDB.", by_ts.len());
                }

                if by_ts.is_empty() {
                    println!("  Keine Daten gefunden.");
                    return Ok(());
                }

                // Build JSON rows: [label, ff, fx, dd, tt, td, rh, qfe, rr, ss, rad]
                let mut json_rows: Vec<String> = Vec::new();
                let mut max_ff = f64::NEG_INFINITY;
                let mut max_ff_time = String::new();
                let mut max_fx = f64::NEG_INFINITY;
                let mut max_fx_time = String::new();
                let mut min_tt = f64::INFINITY;
                let mut min_tt_time = String::new();
                let mut max_tt = f64::NEG_INFINITY;
                let mut max_tt_time = String::new();

                for (ts, vals) in &by_ts {
                    let dt = DateTime::from_timestamp(*ts, 0).unwrap_or(DateTime::<Utc>::MIN_UTC);
                    let label = dt.format("%-d.%-m. %H:%M").to_string();

                    let ff = vals.get("ff").copied().unwrap_or(f64::NAN);
                    let fx = vals.get("fx").copied().unwrap_or(f64::NAN);
                    let dd = vals.get("dd").copied().unwrap_or(f64::NAN);
                    let tt = vals.get("tt").copied().unwrap_or(f64::NAN);
                    let td = vals.get("td").copied().unwrap_or(f64::NAN);
                    let rh = vals.get("rh").copied().unwrap_or(f64::NAN);
                    let qfe = vals.get("qfe").copied().unwrap_or(f64::NAN);
                    let rr = vals.get("rr").copied().unwrap_or(f64::NAN);
                    let ss = vals.get("ss").copied().unwrap_or(f64::NAN);
                    let rad = vals.get("rad").copied().unwrap_or(f64::NAN);

                    fn jv(v: f64) -> String {
                        if v.is_nan() { "null".into() } else { format!("{}", v) }
                    }

                    json_rows.push(format!(
                        "[\"{}\",{},{},{},{},{},{},{},{},{},{}]",
                        label, jv(ff), jv(fx), jv(dd), jv(tt), jv(td), jv(rh), jv(qfe), jv(rr), jv(ss), jv(rad)
                    ));

                    if !ff.is_nan() && ff > max_ff { max_ff = ff; max_ff_time = label.clone(); }
                    if !fx.is_nan() && fx > max_fx { max_fx = fx; max_fx_time = label.clone(); }
                    if !tt.is_nan() && tt < min_tt { min_tt = tt; min_tt_time = label.clone(); }
                    if !tt.is_nan() && tt > max_tt { max_tt = tt; max_tt_time = label.clone(); }
                }

                let output_path = output.unwrap_or_else(|| {
                    format!("html/{}_{}_{}.html", lake_name.to_lowercase(), start, end)
                });
                if let Some(parent) = std::path::Path::new(&output_path).parent() {
                    std::fs::create_dir_all(parent)?;
                }

                let chartjs = include_str!("chartjs.min.js");
                let mut f = std::fs::File::create(&output_path)?;

                write!(f, r#"<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{lake_name} — {start} bis {end}</title>
<script>
{chartjs}
</script>
<style>
  :root {{ --bg: #f8f9fa; --card: #fff; --text: #212529; --muted: #6c757d; --border: #dee2e6; --blue: #0d6efd; --red: #dc3545; --green: #198754; --cyan: #0dcaf0; --orange: #fd7e14; --purple: #6f42c1; }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: var(--bg); color: var(--text); line-height: 1.6; padding: 1rem; }}
  .container {{ max-width: 1400px; margin: 0 auto; }}
  h1 {{ font-size: 1.5rem; margin-bottom: 0.25rem; }}
  .subtitle {{ color: var(--muted); margin-bottom: 0.5rem; font-size: 0.9rem; }}
  .sources {{ color: var(--muted); margin-bottom: 1.5rem; font-size: 0.8rem; background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 0.5rem 1rem; }}
  .sources strong {{ color: var(--text); }}
  .stats {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 0.75rem; margin-bottom: 1.5rem; }}
  .stat {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 0.75rem 1rem; }}
  .stat .label {{ font-size: 0.7rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; }}
  .stat .value {{ font-size: 1.4rem; font-weight: 700; }}
  .stat .unit {{ font-size: 0.8rem; color: var(--muted); }}
  .chart-card {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 1rem; margin-bottom: 1rem; }}
  .chart-card h2 {{ font-size: 1rem; margin-bottom: 0.75rem; }}
  .chart-card .src-note {{ font-size: 0.7rem; color: var(--muted); margin-bottom: 0.5rem; }}
  .chart-wrap {{ position: relative; height: 300px; }}
  .charts-grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }}
  @media (max-width: 900px) {{ .charts-grid {{ grid-template-columns: 1fr; }} }}
  table {{ width: 100%; border-collapse: collapse; font-size: 0.75rem; font-variant-numeric: tabular-nums; }}
  th, td {{ padding: 3px 6px; text-align: right; border-bottom: 1px solid var(--border); white-space: nowrap; }}
  th {{ background: var(--bg); position: sticky; top: 0; font-weight: 600; z-index: 1; }}
  td:first-child, th:first-child {{ text-align: left; }}
  .table-wrap {{ max-height: 600px; overflow: auto; border: 1px solid var(--border); border-radius: 8px; }}
  .day-header {{ background: #e9ecef; font-weight: 700; }}
  footer {{ margin-top: 2rem; padding-top: 1rem; border-top: 1px solid var(--border); font-size: 0.75rem; color: var(--muted); }}
</style>
</head>
<body>
<div class="container">

<h1>{lake_name} — {bafu_desc}</h1>
<p class="subtitle">{start} bis {end}</p>
<div class="sources">
  <strong>MeteoSwiss {smn_station}</strong> ({smn_desc}): Wind, Temperatur, Feuchtigkeit, Druck, Niederschlag, Sonne, Strahlung
</div>

<div class="sources">
  <strong>Messstationen:</strong><br>
  <a href="https://maps.google.com/?q={smn_lat},{smn_lon}" target="_blank" rel="noopener">MeteoSwiss {smn_station} — {smn_desc}</a><br>
  <a href="https://maps.google.com/?q={bafu_lat},{bafu_lon}" target="_blank" rel="noopener">BAFU {bafu_id} — Pegel {bafu_desc}</a>
</div>
{webcams_html}
<div class="stats">
  <div class="stat"><div class="label">Wind Max</div><div class="value" style="color:var(--green)">{max_ff:.1} <span class="unit">km/h</span></div><div class="label">{max_ff_time}</div></div>
  <div class="stat"><div class="label">Böen Max</div><div class="value" style="color:var(--orange)">{max_fx:.1} <span class="unit">km/h</span></div><div class="label">{max_fx_time}</div></div>
  <div class="stat"><div class="label">Temp Min</div><div class="value" style="color:var(--blue)">{min_tt:.1} <span class="unit">&deg;C</span></div><div class="label">{min_tt_time}</div></div>
  <div class="stat"><div class="label">Temp Max</div><div class="value" style="color:var(--red)">{max_tt:.1} <span class="unit">&deg;C</span></div><div class="label">{max_tt_time}</div></div>
</div>

<div class="chart-card">
  <h2>Wind &amp; Böen</h2>
  <div class="src-note">MeteoSwiss {smn_station}</div>
  <div class="chart-wrap"><canvas id="chartWind"></canvas></div>
</div>

<div class="charts-grid">
  <div class="chart-card"><h2>Windrichtung</h2><div class="src-note">{smn_station}</div><div class="chart-wrap"><canvas id="chartWindDir"></canvas></div></div>
  <div class="chart-card"><h2>Temperatur &amp; Taupunkt</h2><div class="src-note">{smn_station}</div><div class="chart-wrap"><canvas id="chartTemp"></canvas></div></div>
</div>

<div class="charts-grid">
  <div class="chart-card"><h2>Luftdruck &amp; Feuchtigkeit</h2><div class="src-note">{smn_station}</div><div class="chart-wrap"><canvas id="chartPressure"></canvas></div></div>
  <div class="chart-card"><h2>Sonnenstrahlung</h2><div class="src-note">{smn_station}</div><div class="chart-wrap"><canvas id="chartRad"></canvas></div></div>
</div>

<div class="chart-card">
  <h2>Alle Messwerte — {count} Datenpunkte</h2>
  <div class="table-wrap">
    <table><thead><tr>
      <th>Zeit</th><th>Wind km/h</th><th>Böen km/h</th><th>Richtung</th><th>Temp &deg;C</th>
      <th>Taupkt &deg;C</th><th>Feuchte %</th><th>Druck hPa</th><th>Regen mm</th><th>Sonne min</th><th>Strahl. W/m&sup2;</th>
    </tr></thead><tbody id="tableBody"></tbody></table>
  </div>
</div>

<footer>MeteoSwiss {smn_station} ({smn_desc}) &middot; Generiert mit <strong>pegelstand</strong> CLI v{version}</footer>
</div>

<script>
// [label, ff, fx, dd, tt, td, rh, qfe, rr, ss, rad]
const data = [
{json_data}
];
"#,
                    start = start, end = end, chartjs = chartjs,
                    lake_name = lake_name, bafu_desc = bafu_desc,
                    smn_station = smn_station, smn_desc = smn_desc,
                    smn_lat = lc.smn_lat, smn_lon = lc.smn_lon,
                    bafu_lat = lc.bafu_lat, bafu_lon = lc.bafu_lon,
                    webcams_html = if lc.webcams.is_empty() { String::new() } else {
                        let mut wh = String::from("<div class=\"sources\">\n  <strong>Webcams:</strong><br>\n");
                        for (label, url) in lc.webcams {
                            wh.push_str(&format!("  <a href=\"{}\" target=\"_blank\" rel=\"noopener\">{}</a><br>\n", url, label));
                        }
                        wh.push_str("</div>\n");
                        wh
                    },
                    max_ff = max_ff, max_ff_time = max_ff_time,
                    max_fx = max_fx, max_fx_time = max_fx_time,
                    min_tt = min_tt, min_tt_time = min_tt_time,
                    max_tt = max_tt, max_tt_time = max_tt_time,
                    count = json_rows.len(), version = APP_VERSION,
                    json_data = json_rows.join(",\n"),
                )?;

                write!(f, r#"
const labels = data.map(d => d[0]);
const wind = data.map(d => d[1]);
const gusts = data.map(d => d[2]);
const windDir = data.map(d => d[3]);
const temp = data.map(d => d[4]);
const dewpt = data.map(d => d[5]);
const humidity = data.map(d => d[6]);
const pressure = data.map(d => d[7]);
const precip = data.map(d => d[8]);
const sun = data.map(d => d[9]);
const radiation = data.map(d => d[10]);

function dirLabel(deg) {{
  if (deg === null) return '-';
  const dirs = ['N','NO','O','SO','S','SW','W','NW'];
  return dirs[Math.round(((deg % 360) + 360) % 360 / 45) % 8];
}}

const co = {{
  responsive: true, maintainAspectRatio: false,
  interaction: {{ mode: 'index', intersect: false }},
  plugins: {{ legend: {{ position: 'top', labels: {{ usePointStyle: true, boxWidth: 8, font: {{ size: 11 }} }} }} }},
  scales: {{ x: {{ ticks: {{ maxTicksLimit: 20, maxRotation: 45, font: {{ size: 10 }} }} }} }},
  elements: {{ point: {{ radius: 0 }}, line: {{ borderWidth: 1.5 }} }},
}};

new Chart(document.getElementById('chartWind'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Wind', data: wind, borderColor: '#198754', fill: false }},
    {{ label: 'Böen', data: gusts, borderColor: '#fd7e14', backgroundColor: 'rgba(253,126,20,0.1)', fill: true }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: 'km/h' }} }} }} }}
}});

new Chart(document.getElementById('chartWindDir'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Windrichtung', data: windDir, borderColor: '#6f42c1', fill: false, pointRadius: 2, pointBackgroundColor: '#6f42c1', showLine: false }},
  ] }}, options: {{ ...co, elements: {{ point: {{ radius: 2 }}, line: {{ borderWidth: 0 }} }},
    scales: {{ ...co.scales, y: {{ min: 0, max: 360, title: {{ display: true, text: 'Grad' }},
      ticks: {{ stepSize: 45, callback: function(v) {{ const d = {{0:'N',45:'NO',90:'O',135:'SO',180:'S',225:'SW',270:'W',315:'NW',360:'N'}}; return d[v] || v+'°'; }} }} }} }} }}
}});

new Chart(document.getElementById('chartTemp'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Temperatur', data: temp, borderColor: '#dc3545', fill: false }},
    {{ label: 'Taupunkt', data: dewpt, borderColor: '#6c757d', borderDash: [4,4], fill: false }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: '°C' }} }} }} }}
}});

new Chart(document.getElementById('chartPressure'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Luftdruck', data: pressure, borderColor: '#6f42c1', yAxisID: 'y', fill: false }},
    {{ label: 'Feuchtigkeit', data: humidity, borderColor: '#0dcaf0', yAxisID: 'y1', fill: false, borderDash: [4,4] }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ position: 'left', title: {{ display: true, text: 'hPa' }} }}, y1: {{ position: 'right', title: {{ display: true, text: '%' }}, grid: {{ drawOnChartArea: false }} }} }} }}
}});

new Chart(document.getElementById('chartRad'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Strahlung', data: radiation, borderColor: '#ffc107', backgroundColor: 'rgba(255,193,7,0.1)', fill: true }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: 'W/m²' }}, min: 0 }} }} }}
}});

const tbody = document.getElementById('tableBody');
let lastDay = '';
const fmt = (v, d) => v === null ? '-' : d === 0 ? Math.round(v).toString() : v.toFixed(d);
data.forEach(d => {{
  const day = d[0].split(' ')[0];
  if (day !== lastDay) {{
    const tr = document.createElement('tr');
    tr.className = 'day-header';
    tr.innerHTML = '<td colspan="11">' + day + '</td>';
    tbody.appendChild(tr);
    lastDay = day;
  }}
  const tr = document.createElement('tr');
  const dirStr = d[3] !== null ? Math.round(d[3]) + '° ' + dirLabel(d[3]) : '-';
  tr.innerHTML = '<td>' + d[0].split(' ')[1] + '</td>'
    + '<td>' + fmt(d[1],1) + '</td><td>' + fmt(d[2],1) + '</td><td>' + dirStr + '</td>'
    + '<td>' + fmt(d[4],1) + '</td><td>' + fmt(d[5],1) + '</td><td>' + fmt(d[6],0) + '</td>'
    + '<td>' + fmt(d[7],1) + '</td><td>' + fmt(d[8],1) + '</td><td>' + fmt(d[9],0) + '</td>'
    + '<td>' + fmt(d[10],0) + '</td>';
  tbody.appendChild(tr);
}});
</script>
</body>
</html>"#)?;

                println!("  {} Datenpunkte geschrieben.", json_rows.len());
                println!("  Datei: {}", output_path);
                return Ok(());
            }

            let start_date = &start;
            let end_date_next = chrono::NaiveDate::parse_from_str(&end, "%Y-%m-%d")
                .map(|d| d.succ_opt().unwrap_or(d).format("%Y-%m-%d").to_string())
                .unwrap_or_else(|_| end.clone());

            // Fetch both stations for the full range, paginating
            println!("  Lade Daten Tiefenbrunnen + Mythenquai ({} bis {})...", start, end);

            let mut tb_all: Vec<TecdottirMeasurement> = Vec::new();
            let mut mq_all: Vec<TecdottirMeasurement> = Vec::new();

            for (station, dest) in [("tiefenbrunnen", &mut tb_all), ("mythenquai", &mut mq_all)] {
                let mut offset = 0u32;
                loop {
                    let url = format!(
                        "{}/measurements/{}?startDate={}&endDate={}&sort=timestamp_cet%20asc&limit=1000&offset={}",
                        TECDOTTIR_URL, station, start_date, end_date_next, offset
                    );
                    let resp: TecdottirResponse = client.get(&url).send().await?.json().await?;
                    let count = resp.result.len();
                    dest.extend(resp.result);
                    if count < 1000 { break; }
                    offset += 1000;
                    if offset > 50000 { break; }
                }
            }

            // Index mythenquai by time key
            let mq_by_time: HashMap<String, &TecdottirMeasurement> = mq_all
                .iter()
                .map(|m| {
                    let key = if m.timestamp.len() >= 16 { m.timestamp[..16].to_string() } else { m.timestamp.clone() };
                    (key, m)
                })
                .collect();

            // Build JSON data array
            let mut json_rows: Vec<String> = Vec::new();
            let mut min_w = f64::INFINITY;
            let mut max_w = f64::NEG_INFINITY;
            let mut min_w_time = String::new();
            let mut max_w_time = String::new();
            let mut min_chill = f64::INFINITY;
            let mut min_chill_time = String::new();
            let mut max_gust = f64::NEG_INFINITY;
            let mut max_gust_time = String::new();
            let mut max_bft = 0u32;
            let mut max_bft_time = String::new();
            let mut min_press = f64::INFINITY;
            let mut min_press_time = String::new();

            for m in &tb_all {
                let ts = if m.timestamp.len() >= 16 { &m.timestamp[..16] } else { &m.timestamp };
                // Format: DD.M. HH:MM
                let label = if ts.len() >= 16 {
                    let date_part = &ts[..10]; // YYYY-MM-DD
                    let time_part = &ts[11..16]; // HH:MM
                    if let Ok(d) = chrono::NaiveDate::parse_from_str(date_part, "%Y-%m-%d") {
                        format!("{}.{}. {}", d.format("%d").to_string().trim_start_matches('0'), d.format("%m").to_string().trim_start_matches('0'), time_part)
                    } else {
                        ts.to_string()
                    }
                } else {
                    ts.to_string()
                };

                let v = &m.values;
                let wt = v.water_temperature.value.unwrap_or(f64::NAN);
                let at = v.air_temperature.value.unwrap_or(f64::NAN);
                let wc = v.windchill.as_ref().and_then(|x| x.value);
                let dp = v.dew_point.as_ref().and_then(|x| x.value);
                let hu = v.humidity.as_ref().and_then(|x| x.value);
                let ws = v.wind_speed_avg_10min.as_ref().and_then(|x| x.value);
                let wg = v.wind_gust_max_10min.as_ref().and_then(|x| x.value);
                let wf = v.wind_force_avg_10min.as_ref().and_then(|x| x.value);
                let wd = v.wind_direction.as_ref().and_then(|x| x.value);
                let bp = v.barometric_pressure_qfe.as_ref().and_then(|x| x.value);

                let time_key = if m.timestamp.len() >= 16 { m.timestamp[..16].to_string() } else { m.timestamp.clone() };
                let (pr, gr, wl) = if let Some(mq) = mq_by_time.get(&time_key) {
                    let mv = &mq.values;
                    (
                        mv.precipitation.as_ref().and_then(|x| x.value),
                        mv.global_radiation.as_ref().and_then(|x| x.value),
                        mv.water_level.as_ref().and_then(|x| x.value),
                    )
                } else {
                    (None, None, None)
                };

                fn jv(v: Option<f64>) -> String {
                    match v { Some(x) if !x.is_nan() => format!("{}", x), _ => "null".into() }
                }
                fn jvf(v: f64) -> String {
                    if v.is_nan() { "null".into() } else { format!("{}", v) }
                }

                json_rows.push(format!(
                    "[\"{}\",{},{},{},{},{},{},{},{},{},{},{},{},{},\"{}\"]",
                    label, jvf(wt), jvf(at), jv(wc), jv(dp), jv(hu), jv(ws), jv(wg), jv(wf), jv(wd), jv(bp), jv(pr), jv(gr), jv(wl),
                    m.station
                ));

                // Track stats
                if !wt.is_nan() {
                    if wt < min_w { min_w = wt; min_w_time = label.clone(); }
                    if wt > max_w { max_w = wt; max_w_time = label.clone(); }
                }
                if let Some(c) = wc { if c < min_chill { min_chill = c; min_chill_time = label.clone(); } }
                if let Some(g) = wg { if g > max_gust { max_gust = g; max_gust_time = label.clone(); } }
                if let Some(b) = wf { let bi = b as u32; if bi > max_bft { max_bft = bi; max_bft_time = label.clone(); } }
                if let Some(p) = bp { if p < min_press { min_press = p; min_press_time = label.clone(); } }
            }

            if json_rows.is_empty() {
                println!("  Keine Daten gefunden.");
                return Ok(());
            }

            let suffix = if svg { "_svg" } else { "" };
            let output_path = output.unwrap_or_else(|| {
                format!("html/{}_{}{}.html", start, end, suffix)
            });

            if let Some(parent) = std::path::Path::new(&output_path).parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut f = std::fs::File::create(&output_path)?;

            if svg {
                svg_report::write_svg_report(
                    &mut f, &start, &end, &json_rows,
                    min_w, &min_w_time, max_w, &max_w_time,
                    min_chill, &min_chill_time, max_gust, &max_gust_time,
                    max_bft, &max_bft_time, min_press, &min_press_time,
                    APP_VERSION,
                )?;
                println!("  {} Datenpunkte geschrieben (SVG).", json_rows.len());
                println!("  Datei: {}", output_path);
                return Ok(());
            }

            let chartjs = include_str!("chartjs.min.js");

            write!(f, r#"<!DOCTYPE html>
<html lang="de">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Zürichsee — {start} bis {end}</title>
<script>
{chartjs}
</script>
<style>
  :root {{ --bg: #f8f9fa; --card: #fff; --text: #212529; --muted: #6c757d; --border: #dee2e6; --blue: #0d6efd; --red: #dc3545; --green: #198754; --cyan: #0dcaf0; --orange: #fd7e14; --purple: #6f42c1; }}
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; background: var(--bg); color: var(--text); line-height: 1.6; padding: 1rem; }}
  .container {{ max-width: 1400px; margin: 0 auto; }}
  h1 {{ font-size: 1.5rem; margin-bottom: 0.25rem; }}
  .subtitle {{ color: var(--muted); margin-bottom: 0.5rem; font-size: 0.9rem; }}
  .sources {{ color: var(--muted); margin-bottom: 1.5rem; font-size: 0.8rem; background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 0.5rem 1rem; }}
  .sources strong {{ color: var(--text); }}
  .stats {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 0.75rem; margin-bottom: 1.5rem; }}
  .stat {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 0.75rem 1rem; }}
  .stat .label {{ font-size: 0.7rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; }}
  .stat .value {{ font-size: 1.4rem; font-weight: 700; }}
  .stat .unit {{ font-size: 0.8rem; color: var(--muted); }}
  .stat .src {{ font-size: 0.65rem; color: var(--muted); }}
  .chart-card {{ background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 1rem; margin-bottom: 1rem; }}
  .chart-card h2 {{ font-size: 1rem; margin-bottom: 0.75rem; }}
  .chart-card .src-note {{ font-size: 0.7rem; color: var(--muted); margin-bottom: 0.5rem; }}
  .chart-wrap {{ position: relative; height: 300px; }}
  .charts-grid {{ display: grid; grid-template-columns: 1fr 1fr; gap: 1rem; }}
  @media (max-width: 900px) {{ .charts-grid {{ grid-template-columns: 1fr; }} }}
  table {{ width: 100%; border-collapse: collapse; font-size: 0.75rem; font-variant-numeric: tabular-nums; }}
  th, td {{ padding: 3px 6px; text-align: right; border-bottom: 1px solid var(--border); white-space: nowrap; }}
  th {{ background: var(--bg); position: sticky; top: 0; font-weight: 600; z-index: 1; }}
  th .thsrc {{ font-weight: 400; color: var(--muted); font-size: 0.65rem; }}
  td:first-child, th:first-child {{ text-align: left; }}
  .table-wrap {{ max-height: 600px; overflow: auto; border: 1px solid var(--border); border-radius: 8px; }}
  .day-header {{ background: #e9ecef; font-weight: 700; }}
  footer {{ margin-top: 2rem; padding-top: 1rem; border-top: 1px solid var(--border); font-size: 0.75rem; color: var(--muted); }}
</style>
</head>
<body>
<div class="container">

<h1>Zürichsee — Kombinierte Messwerte</h1>
<p class="subtitle">{start} bis {end}</p>
<div class="sources">
  <strong>Tiefenbrunnen (T):</strong> Wassertemp, Lufttemp, Windchill, Taupunkt, Feuchtigkeit, Wind, Böen, Beaufort, Windrichtung, Luftdruck<br>
  <strong>Mythenquai (M):</strong> Niederschlag, Sonnenstrahlung, Pegel<br>
  Quelle: Wasserschutzpolizei Zürich (tecdottir.metaodi.ch) &amp; BAFU (api.existenz.ch)
</div>

<div class="sources">
  <strong>Messstationen:</strong><br>
  <a href="https://maps.google.com/?q=47.3505,8.5583" target="_blank" rel="noopener">Tiefenbrunnen (T) — Zürichsee, Wassertemp &amp; Wind</a><br>
  <a href="https://maps.google.com/?q=47.3545,8.5366" target="_blank" rel="noopener">Mythenquai (M) — Zürichsee, Niederschlag &amp; Pegel</a><br>
  <a href="https://maps.google.com/?q=47.3548,8.5505" target="_blank" rel="noopener">BAFU 2209 — Pegel Zürichsee</a>
</div>

<div class="sources">
  <strong>Webcams:</strong><br>
  <a href="https://zuerichtourismus.roundshot.com/" target="_blank" rel="noopener">Roundshot Zürich Tourismus (360°)</a><br>
  <a href="https://www.stadt-zuerich.ch/pd/de/index/stadtpolizei_zuerich/gewaesser/wetterstationen_webcam/webcam_wapo.html" target="_blank" rel="noopener">Webcam Wasserschutzpolizei Mythenquai</a>
</div>

<div class="stats">
  <div class="stat"><div class="label">Wassertemp Min</div><div class="value" style="color:var(--blue)">{min_w:.1} <span class="unit">&deg;C</span></div><div class="label">{min_w_time}</div><div class="src">Tiefenbrunnen</div></div>
  <div class="stat"><div class="label">Wassertemp Max</div><div class="value" style="color:var(--red)">{max_w:.1} <span class="unit">&deg;C</span></div><div class="label">{max_w_time}</div><div class="src">Tiefenbrunnen</div></div>
  <div class="stat"><div class="label">Windchill Min</div><div class="value" style="color:var(--cyan)">{min_chill:.1} <span class="unit">&deg;C</span></div><div class="label">{min_chill_time}</div><div class="src">Tiefenbrunnen</div></div>
  <div class="stat"><div class="label">Böen Max</div><div class="value" style="color:var(--orange)">{max_gust:.1} <span class="unit">m/s</span></div><div class="label">{max_gust_time}</div><div class="src">Tiefenbrunnen</div></div>
  <div class="stat"><div class="label">Beaufort Max</div><div class="value" style="color:var(--orange)">{max_bft} <span class="unit">bft</span></div><div class="label">{max_bft_time}</div><div class="src">Tiefenbrunnen</div></div>
  <div class="stat"><div class="label">Luftdruck Min</div><div class="value" style="color:var(--purple)">{min_press:.0} <span class="unit">hPa</span></div><div class="label">{min_press_time}</div><div class="src">Tiefenbrunnen</div></div>
</div>

<div class="chart-card">
  <h2>Temperaturverlauf</h2>
  <div class="src-note">Wassertemp / Lufttemp / Windchill / Taupunkt: Tiefenbrunnen (T)</div>
  <div class="chart-wrap"><canvas id="chartTemp"></canvas></div>
</div>

<div class="charts-grid">
  <div class="chart-card"><h2>Wind, Böen &amp; Beaufort</h2><div class="src-note">Tiefenbrunnen (T)</div><div class="chart-wrap"><canvas id="chartWind"></canvas></div></div>
  <div class="chart-card"><h2>Windrichtung</h2><div class="src-note">Tiefenbrunnen (T)</div><div class="chart-wrap"><canvas id="chartWindDir"></canvas></div></div>
</div>

<div class="charts-grid">
  <div class="chart-card"><h2>Luftdruck &amp; Feuchtigkeit</h2><div class="src-note">Tiefenbrunnen (T)</div><div class="chart-wrap"><canvas id="chartPressure"></canvas></div></div>
  <div class="chart-card"><h2>Pegel</h2><div class="src-note">Mythenquai (M)</div><div class="chart-wrap"><canvas id="chartPegel"></canvas></div></div>
</div>

<div class="chart-card">
  <h2>Alle Messwerte (10-Minuten-Intervall) — {count} Datenpunkte</h2>
  <div class="table-wrap">
    <table><thead><tr>
      <th>Zeit</th>
      <th>Wasser &deg;C <span class="thsrc">(T)</span></th>
      <th>Luft &deg;C <span class="thsrc">(T)</span></th>
      <th>Chill &deg;C <span class="thsrc">(T)</span></th>
      <th>Taupkt &deg;C <span class="thsrc">(T)</span></th>
      <th>Feuchte % <span class="thsrc">(T)</span></th>
      <th>Wind m/s <span class="thsrc">(T)</span></th>
      <th>Böen m/s <span class="thsrc">(T)</span></th>
      <th>Bft <span class="thsrc">(T)</span></th>
      <th>Ri &deg; <span class="thsrc">(T)</span></th>
      <th>Druck hPa <span class="thsrc">(T)</span></th>
      <th>Regen mm <span class="thsrc">(M)</span></th>
      <th>Strahl. W/m&sup2; <span class="thsrc">(M)</span></th>
      <th>Pegel m <span class="thsrc">(M)</span></th>
    </tr></thead><tbody id="tableBody"></tbody></table>
  </div>
</div>

<footer>
  Tiefenbrunnen (T) &amp; Mythenquai (M) — Wasserschutzpolizei Zürich (tecdottir.metaodi.ch) &middot; Generiert mit <strong>pegelstand</strong> CLI v{version}
</footer>

</div>

<script>
const data = [
{json_data}
];
"#,
                start = start,
                end = end,
                chartjs = chartjs,
                min_w = min_w,
                min_w_time = min_w_time,
                max_w = max_w,
                max_w_time = max_w_time,
                min_chill = min_chill,
                min_chill_time = min_chill_time,
                max_gust = max_gust,
                max_gust_time = max_gust_time,
                max_bft = max_bft,
                max_bft_time = max_bft_time,
                min_press = min_press,
                min_press_time = min_press_time,
                count = json_rows.len(),
                version = APP_VERSION,
                json_data = json_rows.join(",\n"),
            )?;

            write!(f, r#"
const labels = data.map(d => d[0]);
const waterTemp = data.map(d => d[1]);
const airTemp = data.map(d => d[2]);
const windchill = data.map(d => d[3]);
const dewpoint = data.map(d => d[4]);
const humidity = data.map(d => d[5]);
const wind = data.map(d => d[6]);
const gusts = data.map(d => d[7]);
const bft = data.map(d => d[8]);
const windDir = data.map(d => d[9]);
const pressure = data.map(d => d[10]);
const precip = data.map(d => d[11]);
const radiation = data.map(d => d[12]);
const waterLevel = data.map(d => d[13]);

function dirLabel(deg) {{
  if (deg === null) return '-';
  const dirs = ['N','NO','O','SO','S','SW','W','NW'];
  return dirs[Math.round(((deg % 360) + 360) % 360 / 45) % 8];
}}

const co = {{
  responsive: true, maintainAspectRatio: false,
  interaction: {{ mode: 'index', intersect: false }},
  plugins: {{ legend: {{ position: 'top', labels: {{ usePointStyle: true, boxWidth: 8, font: {{ size: 11 }} }} }} }},
  scales: {{ x: {{ ticks: {{ maxTicksLimit: 20, maxRotation: 45, font: {{ size: 10 }} }} }} }},
  elements: {{ point: {{ radius: 0 }}, line: {{ borderWidth: 1.5 }} }},
}};

new Chart(document.getElementById('chartTemp'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Wassertemp (T)', data: waterTemp, borderColor: '#0d6efd', backgroundColor: 'rgba(13,110,253,0.08)', fill: true }},
    {{ label: 'Lufttemp (T)', data: airTemp, borderColor: '#dc3545', fill: false }},
    {{ label: 'Windchill (T)', data: windchill, borderColor: '#0dcaf0', borderDash: [3,3], fill: false }},
    {{ label: 'Taupunkt (T)', data: dewpoint, borderColor: '#6c757d', borderDash: [6,3], fill: false }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: '°C' }} }} }} }}
}});

new Chart(document.getElementById('chartWind'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Wind 10min (T)', data: wind, borderColor: '#198754', yAxisID: 'y', fill: false }},
    {{ label: 'Böen max (T)', data: gusts, borderColor: '#fd7e14', backgroundColor: 'rgba(253,126,20,0.1)', yAxisID: 'y', fill: true }},
    {{ label: 'Beaufort (T)', data: bft, borderColor: '#d63384', borderDash: [4,4], yAxisID: 'y1', fill: false }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ position: 'left', title: {{ display: true, text: 'm/s' }} }}, y1: {{ position: 'right', title: {{ display: true, text: 'bft' }}, grid: {{ drawOnChartArea: false }}, min: 0, max: 12 }} }} }}
}});

new Chart(document.getElementById('chartWindDir'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Windrichtung (T)', data: windDir, borderColor: '#6f42c1', fill: false, pointRadius: 2, pointBackgroundColor: '#6f42c1', showLine: false }},
  ] }}, options: {{ ...co, elements: {{ point: {{ radius: 2 }}, line: {{ borderWidth: 0 }} }},
    scales: {{ ...co.scales, y: {{ min: 0, max: 360, title: {{ display: true, text: 'Grad' }}, ticks: {{ stepSize: 45, callback: function(v) {{ const d = {{0:'N',45:'NO',90:'O',135:'SO',180:'S',225:'SW',270:'W',315:'NW',360:'N'}}; return d[v] || v+'°'; }} }} }} }} }}
}});

new Chart(document.getElementById('chartPressure'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Luftdruck (T)', data: pressure, borderColor: '#6f42c1', yAxisID: 'y', fill: false }},
    {{ label: 'Feuchtigkeit (T)', data: humidity, borderColor: '#0dcaf0', yAxisID: 'y1', fill: false, borderDash: [4,4] }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ position: 'left', title: {{ display: true, text: 'hPa' }} }}, y1: {{ position: 'right', title: {{ display: true, text: '%' }}, grid: {{ drawOnChartArea: false }} }} }} }}
}});

new Chart(document.getElementById('chartPegel'), {{
  type: 'line', data: {{ labels, datasets: [
    {{ label: 'Pegel (M)', data: waterLevel, borderColor: '#198754', backgroundColor: 'rgba(25,135,84,0.1)', fill: true }},
  ] }}, options: {{ ...co, scales: {{ ...co.scales, y: {{ title: {{ display: true, text: 'm ü.M.' }} }} }} }}
}});

const tbody = document.getElementById('tableBody');
let lastDay = '';
const fmt = (v, d) => v === null ? '-' : d === 0 ? Math.round(v).toString() : v.toFixed(d);
data.forEach(d => {{
  const day = d[0].split(' ')[0];
  if (day !== lastDay) {{
    const tr = document.createElement('tr');
    tr.className = 'day-header';
    tr.innerHTML = '<td colspan="14">' + day + '</td>';
    tbody.appendChild(tr);
    lastDay = day;
  }}
  const tr = document.createElement('tr');
  const dirStr = d[9] !== null ? Math.round(d[9]) + '° ' + dirLabel(d[9]) : '-';
  tr.innerHTML = '<td>' + d[0].split(' ')[1] + '</td>'
    + '<td>' + fmt(d[1],1) + '</td><td>' + fmt(d[2],1) + '</td><td>' + fmt(d[3],1) + '</td>'
    + '<td>' + fmt(d[4],1) + '</td><td>' + fmt(d[5],0) + '</td><td>' + fmt(d[6],1) + '</td>'
    + '<td>' + fmt(d[7],1) + '</td><td>' + fmt(d[8],0) + '</td><td>' + dirStr + '</td>'
    + '<td>' + fmt(d[10],0) + '</td><td>' + fmt(d[11],1) + '</td><td>' + fmt(d[12],0) + '</td>'
    + '<td>' + fmt(d[13],1) + '</td>';
  tbody.appendChild(tr);
}});
</script>
</body>
</html>"#)?;

            println!("  {} Datenpunkte geschrieben.", json_rows.len());
            println!("  Datei: {}", output_path);
        }
    }

    Ok(())
}
