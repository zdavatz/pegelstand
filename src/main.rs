use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use serde::de::{self, Deserializer};
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt;

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

fn print_opt(label: &str, val: &Option<TecdottirValue>, unit: &str, decimals: u8) {
    if let Some(v) = val {
        if let Some(x) = v.value {
            match decimals {
                0 => println!("  {:<18} {:>6.0} {}", label, x, unit),
                _ => println!("  {:<18} {:>6.1} {}", label, x, unit),
            }
        }
    }
}

fn print_opt_wind_dir(val: &Option<TecdottirValue>) {
    if let Some(v) = val {
        if let Some(x) = v.value {
            println!("  {:<18} {:>6.0}° ({})", "Windrichtung:", x, wind_direction_label(x));
        }
    }
}

// --- Tecdottir (Zürichsee Temperatur) ---

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
                // All 10-minute data points for a single day
                let next_day = chrono::NaiveDate::parse_from_str(tag, "%Y-%m-%d")
                    .map(|d| d.succ_opt().unwrap_or(d).format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|_| tag.clone());

                let url = format!(
                    "{}/measurements/{}?startDate={}&endDate={}&sort=timestamp_cet%20asc&limit=1000",
                    TECDOTTIR_URL, station, tag, next_day
                );
                let resp: TecdottirResponse = client.get(&url).send().await?.json().await?;

                println!(
                    "\n Zürichsee — {} — alle Messwerte {}",
                    station, tag
                );
                println!("  Quelle: Wasserschutzpolizei Zürich\n");

                if resp.result.is_empty() {
                    println!("  Keine Daten für diesen Tag.");
                    return Ok(());
                }

                println!(
                    "  {:<6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>5} {:>5}",
                    "Zeit", "Wass.", "Luft", "Chill", "Tau", "Feu%", "Wind", "Böen", "Bft", "Ri°", "Druck", "Regen", "Sonn", "WLvl"
                );
                println!(
                    "  {:<6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>5} {:>5}",
                    "", "°C", "°C", "°C", "°C", "%", "m/s", "m/s", "", "", "hPa", "mm", "W/m²", "m"
                );
                println!("  {}", "-".repeat(100));

                let mut min_w = f64::INFINITY;
                let mut max_w = f64::NEG_INFINITY;
                let mut sum_w = 0.0;
                let mut count = 0usize;

                for m in &resp.result {
                    let time = if m.timestamp.len() >= 16 {
                        &m.timestamp[11..16]
                    } else {
                        &m.timestamp
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
                    let pr = v.precipitation.as_ref().and_then(|x| x.value);
                    let gr = v.global_radiation.as_ref().and_then(|x| x.value);
                    let wl = v.water_level.as_ref().and_then(|x| x.value);

                    println!(
                        "  {:<6} {:>6} {:>6} {:>6} {:>5} {:>5} {:>5} {:>4} {:>4} {:>6} {:>6} {:>5} {:>5} {:>5}",
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
                    println!("\n  {}", "-".repeat(72));
                    println!(
                        "  Wassertemperatur: Min {:.1}°C | Max {:.1}°C | Durchschnitt {:.1}°C",
                        min_w, max_w, avg_w
                    );
                    println!("  Messpunkte: {} (alle 10 Minuten)", count);
                }
                println!();
            } else if aktuell {
                // Fetch latest value (look back 2 days to be safe)
                let start_date = (now - chrono::Duration::days(2))
                    .format("%Y-%m-%d")
                    .to_string();
                let url = format!(
                    "{}/measurements/{}?startDate={}&endDate={}&sort=timestamp_cet%20desc&limit=1",
                    TECDOTTIR_URL, station, start_date, end_date
                );
                let resp: TecdottirResponse = client.get(&url).send().await?.json().await?;
                let measurements = resp.result;

                println!(
                    "\n Zürichsee Wassertemperatur — {}",
                    station
                );
                println!("  Quelle: Stadt Zürich / Wasserschutzpolizei\n");

                if let Some(m) = measurements.last() {
                    let v = &m.values;
                    println!("  Zeitpunkt:        {}", &m.timestamp[..19]);
                    println!("  {}", "-".repeat(40));
                    println!("  {:<18} {:>6.1} °C", "Wassertemp:", v.water_temperature.value.unwrap_or(f64::NAN));
                    println!("  {:<18} {:>6.1} °C", "Lufttemp:", v.air_temperature.value.unwrap_or(f64::NAN));
                    print_opt("Windchill:", &v.windchill, "°C", 1);
                    print_opt("Taupunkt:", &v.dew_point, "°C", 1);
                    print_opt("Feuchtigkeit:", &v.humidity, "%", 0);
                    print_opt("Wind (10min):", &v.wind_speed_avg_10min, "m/s", 1);
                    print_opt("Böen (max):", &v.wind_gust_max_10min, "m/s", 1);
                    print_opt("Windstärke:", &v.wind_force_avg_10min, "bft", 0);
                    print_opt_wind_dir(&v.wind_direction);
                    print_opt("Luftdruck:", &v.barometric_pressure_qfe, "hPa", 0);
                    print_opt("Niederschlag:", &v.precipitation, "mm", 1);
                    print_opt("Strahlung:", &v.global_radiation, "W/m²", 0);
                    print_opt("Wasserstand:", &v.water_level, "m", 2);
                } else {
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
    }

    Ok(())
}
