// IMIS-Station — PDF-Steckbrief einer SLF-Messstation mit Lagekarte.
//
// Holt Stammdaten + die Messwerte der letzten 24 h aus der öffentlichen
// SLF-Measurement-API und zeichnet die Station auf zwei Ausschnitte der
// swisstopo-Landeskarte (mit Höhenkurven): Detail (Zoom 15) und Übersicht
// (Zoom 13, mit allen IMIS-Stationen im Ausschnitt — im Val Müstair also auch
// OFE1 Piz Dora). Kacheln aus dem öffentlichen geo.admin.ch-WMTS (Web
// Mercator), eine Zoomstufe feiner geholt als dargestellt → scharf bei 2×.
//
//   cargo run --release --bin imisstation              # OFE2 Murtaröl, Val Müstair
//   cargo run --release --bin imisstation -- OFE1      # andere IMIS-Station als Hauptstation
//   cargo run --release --bin imisstation -- OFE2 --out schnee/x.pdf

use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use base64::Engine;
use genpdf::elements::{Break, Image, Paragraph};
use genpdf::style::{Color, Style};
use genpdf::Alignment;

const DEFAULT_FONT_DIR: &str = "/usr/share/fonts/dejavu";
const OUT_DIR: &str = "schnee";
const API: &str = "https://measurement-api.slf.ch/public/api/imis";
const TILE: f64 = 256.0;
// Kartenausschnitt in Kartenpixeln (gerendert 2×) → im PDF 170 × 80 mm.
const MAP_W: f64 = 900.0;
const MAP_H: f64 = 424.0;

const INK: Color = Color::Rgb(0x1a, 0x1a, 0x1a);
const ACCENT: Color = Color::Rgb(0x0d, 0x47, 0x6b);
const GOLD: Color = Color::Rgb(0x9a, 0x7b, 0x2e);
const GREY: Color = Color::Rgb(0x55, 0x55, 0x55);

struct Station {
    code: String,
    label: String,
    lat: f64,
    lon: f64,
    elevation: f64,
    kind: String,
    canton: String,
}

/// Ortsangabe für bekannte Stationen (die SLF-API liefert nur den Stationsnamen).
fn region(code: &str) -> &'static str {
    match code {
        "OFE2" => "bei Buffalora, Ofenpass",
        "OFE1" => "Piz Dora, ob Fuldera / Val Müstair",
        _ => "IMIS-Messnetz",
    }
}

// ---- Geo ------------------------------------------------------------------

/// Web-Mercator: (lon,lat) → globale Pixelkoordinate bei Zoom z (256er-Kacheln).
fn lonlat_to_px(lon: f64, lat: f64, z: u32) -> (f64, f64) {
    let n = 2f64.powi(z as i32);
    let x = (lon + 180.0) / 360.0 * n * TILE;
    let lr = lat.to_radians();
    let y = (1.0 - (lr.tan() + 1.0 / lr.cos()).ln() / PI) / 2.0 * n * TILE;
    (x, y)
}

fn meters_per_px(lat: f64, z: u32) -> f64 {
    156_543.033_928_041 * lat.to_radians().cos() / 2f64.powi(z as i32)
}

/// WGS84 → LV95 (swisstopo-Näherungsformel, Genauigkeit ~1 m).
fn wgs84_to_lv95(lat: f64, lon: f64) -> (f64, f64) {
    let p = (lat * 3600.0 - 169_028.66) / 10_000.0;
    let l = (lon * 3600.0 - 26_782.5) / 10_000.0;
    let e = 2_600_072.37 + 211_455.93 * l - 10_938.51 * l * p - 0.36 * l * p * p - 44.54 * l.powi(3);
    let n = 1_200_147.07 + 308_807.95 * p + 3_745.25 * l * l + 76.63 * p * p - 194.56 * l * l * p
        + 119.79 * p.powi(3);
    (e, n)
}

// ---- Karte ----------------------------------------------------------------

async fn fetch_tile(client: &reqwest::Client, z: u32, x: i64, y: i64) -> Option<Vec<u8>> {
    let url = format!(
        "https://wmts.geo.admin.ch/1.0.0/ch.swisstopo.pixelkarte-farbe/default/current/3857/{z}/{x}/{y}.jpeg"
    );
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.bytes().await.ok().map(|b| b.to_vec())
}

/// Kartenausschnitt um die Station: Landeskarte-Kacheln + Marker + Massstab.
/// Gibt RGB-Pixel (2× gerendert) zurück.
/// `marks[0]` ist die Hauptstation (rot), weitere Stationen blau.
async fn render_map(
    client: &reqwest::Client,
    center: (f64, f64),
    marks: &[&Station],
    zoom: u32,
    caption: &str,
) -> Result<(Vec<u8>, u32, u32)> {
    let (sx, sy) = lonlat_to_px(center.1, center.0, zoom);
    let cx0 = sx - MAP_W / 2.0;
    let cy0 = sy - MAP_H / 2.0;
    // Kacheln eine Stufe feiner (halbe Grösse) → beim 2×-Rendern scharf.
    let tz = zoom + 1;
    let n = 2i64.pow(tz);
    let half = TILE / 2.0;
    let engine = base64::engine::general_purpose::STANDARD;

    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{MAP_W}\" height=\"{MAP_H}\" viewBox=\"0 0 {MAP_W} {MAP_H}\">\n\
         <rect width=\"{MAP_W}\" height=\"{MAP_H}\" fill=\"#eeeeee\"/>\n"
    );
    for tx in (cx0 / half).floor() as i64..=((cx0 + MAP_W) / half).floor() as i64 {
        for ty in (cy0 / half).floor() as i64..=((cy0 + MAP_H) / half).floor() as i64 {
            if tx < 0 || ty < 0 || tx >= n || ty >= n {
                continue;
            }
            if let Some(bytes) = fetch_tile(client, tz, tx, ty).await {
                s.push_str(&format!(
                    "<image x=\"{:.1}\" y=\"{:.1}\" width=\"{half}\" height=\"{half}\" href=\"data:image/jpeg;base64,{}\"/>\n",
                    tx as f64 * half - cx0,
                    ty as f64 * half - cy0,
                    engine.encode(&bytes)
                ));
            }
        }
    }

    // Stationen: Punkt mit weissem Rand + Beschriftung (Haupt rot, andere blau).
    for (i, st) in marks.iter().enumerate() {
        let (px, py) = lonlat_to_px(st.lon, st.lat, zoom);
        let (mx, my) = (px - cx0, py - cy0);
        if mx < 0.0 || my < 0.0 || mx > MAP_W || my > MAP_H {
            continue;
        }
        let col = if i == 0 { "#d7191c" } else { "#1f5fbf" };
        s.push_str(&format!(
            "<circle cx=\"{mx:.1}\" cy=\"{my:.1}\" r=\"13\" fill=\"none\" stroke=\"{col}\" stroke-width=\"2.5\" opacity=\"0.6\"/>\n\
             <circle cx=\"{mx:.1}\" cy=\"{my:.1}\" r=\"6.5\" fill=\"{col}\" stroke=\"#ffffff\" stroke-width=\"2.5\"/>\n\
             <g font-family=\"DejaVu Sans, sans-serif\" font-weight=\"bold\" font-size=\"15\">\n\
             <text x=\"{tx:.1}\" y=\"{ty:.1}\" stroke=\"#ffffff\" stroke-width=\"4\" stroke-linejoin=\"round\" fill=\"#ffffff\">{lbl}</text>\n\
             <text x=\"{tx:.1}\" y=\"{ty:.1}\" fill=\"{col}\">{lbl}</text>\n</g>\n",
            tx = mx + 18.0,
            ty = my - 10.0,
            lbl = format!("{} {} · {:.0} m", st.code, st.label, st.elevation),
        ));
    }

    // Massstab unten links: runde Länge ~1/4 der Kartenbreite.
    let mpp = meters_per_px(center.0, zoom);
    let target = MAP_W / 4.0 * mpp;
    let nice = [50.0, 100.0, 200.0, 250.0, 500.0, 1000.0, 2000.0, 2500.0, 5000.0, 10000.0, 20000.0]
        .into_iter()
        .rev()
        .find(|&m| m <= target)
        .unwrap_or(50.0);
    let bar = nice / mpp;
    let label = if nice >= 1000.0 { format!("{} km", nice / 1000.0) } else { format!("{nice} m") };
    let (bx, by) = (16.0, MAP_H - 18.0);
    s.push_str(&format!(
        "<rect x=\"{x0}\" y=\"{y0}\" width=\"{w}\" height=\"30\" fill=\"#ffffff\" opacity=\"0.85\" rx=\"3\"/>\n\
         <rect x=\"{bx}\" y=\"{by}\" width=\"{bar:.1}\" height=\"6\" fill=\"#1a1a1a\"/>\n\
         <rect x=\"{bx}\" y=\"{by}\" width=\"{half:.1}\" height=\"6\" fill=\"#ffffff\" stroke=\"#1a1a1a\" stroke-width=\"1\"/>\n\
         <text x=\"{bx}\" y=\"{ty}\" font-family=\"DejaVu Sans, sans-serif\" font-size=\"12\" fill=\"#1a1a1a\">{label}</text>\n",
        x0 = bx - 8.0,
        y0 = by - 20.0,
        w = bar + 16.0,
        half = bar / 2.0,
        ty = by - 6.0,
    ));
    // Titel oben links.
    s.push_str(&format!(
        "<rect x=\"8\" y=\"8\" width=\"{w:.0}\" height=\"24\" fill=\"#ffffff\" opacity=\"0.85\" rx=\"3\"/>\n\
         <text x=\"16\" y=\"25\" font-family=\"DejaVu Sans, sans-serif\" font-size=\"13\" font-weight=\"bold\" fill=\"#0d476b\">{caption}</text>\n",
        w = caption.chars().count() as f64 * 8.2 + 16.0,
    ));
    s.push_str("</svg>\n");

    let scale = 2.0f32;
    let (ow, oh) = ((MAP_W * 2.0) as u32, (MAP_H * 2.0) as u32);
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(s.as_bytes(), &opt).map_err(|e| anyhow!("SVG parse: {e}"))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ow, oh).ok_or_else(|| anyhow!("Pixmap fehlgeschlagen"))?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Ok((pixmap.data().to_vec(), ow, oh))
}

fn save_rgb(rgba: Vec<u8>, w: u32, h: u32, path: &Path) -> Result<()> {
    let img = image::RgbaImage::from_raw(w, h, rgba).ok_or_else(|| anyhow!("RGBA-Puffergrösse passt nicht"))?;
    image::DynamicImage::ImageRgba8(img).into_rgb8().save(path)?;
    Ok(())
}

// ---- Messwerte ------------------------------------------------------------

/// Kennzahlen der letzten 24 h als (Label, Wert)-Zeilen.
fn summary(rows: &[serde_json::Value]) -> (String, Vec<(String, String)>) {
    let f = |r: &serde_json::Value, k: &str| r[k].as_f64();
    let when = |r: &serde_json::Value| {
        r["measure_date"]
            .as_str()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&chrono::Local).format("%d.%m.%Y %H:%M").to_string())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    let Some(last) = rows.last() else { return (String::new(), out) };
    let hs: Vec<(f64, &serde_json::Value)> = rows.iter().filter_map(|r| f(r, "HS").map(|v| (v, r))).collect();
    if let (Some(&(h1, r1)), Some(&(hn, _))) = (hs.first(), hs.last()) {
        out.push(("Schneehöhe".into(), format!("{hn:.0} cm")));
        out.push(("Änderung 24 h".into(), format!("{:+.0} cm (seit {})", hn - h1, when(r1))));
        let min = hs.iter().map(|x| x.0).fold(f64::INFINITY, f64::min);
        let max = hs.iter().map(|x| x.0).fold(f64::NEG_INFINITY, f64::max);
        out.push(("Min / Max 24 h".into(), format!("{min:.0} / {max:.0} cm")));
    }
    if let Some(v) = f(last, "TA_30MIN_MEAN") { out.push(("Lufttemperatur".into(), format!("{v:.1} °C"))); }
    if let Some(v) = f(last, "TSS_30MIN_MEAN") { out.push(("Schneeoberfläche".into(), format!("{v:.1} °C"))); }
    if let Some(v) = f(last, "RH_30MIN_MEAN") { out.push(("Luftfeuchtigkeit".into(), format!("{v:.0} %"))); }
    if let Some(v) = f(last, "VW_30MIN_MEAN") { out.push(("Wind".into(), format!("{:.0} km/h", v * 3.6))); }
    if let Some(v) = f(last, "VW_30MIN_MAX") { out.push(("Böen".into(), format!("{:.0} km/h", v * 3.6))); }
    if let Some(v) = f(last, "DW_30MIN_MEAN") { out.push(("Windrichtung".into(), format!("{v:.0}°"))); }
    (when(last), out)
}

// ---- PDF ------------------------------------------------------------------

fn build_pdf(
    font_dir: &str,
    st: &Station,
    maps: &[(PathBuf, u32, u32)],
    measured_at: &str,
    values: &[(String, String)],
    neighbours: &[(&Station, String, Vec<(String, String)>)],
    out: &Path,
) -> Result<()> {
    let load = |file: &str| -> Result<genpdf::fonts::FontData> {
        let p = Path::new(font_dir).join(file);
        let data = std::fs::read(&p).map_err(|e| anyhow!("read font {}: {e}", p.display()))?;
        genpdf::fonts::FontData::new(data, None).map_err(|e| anyhow!("parse font {file}: {e}"))
    };
    let family = genpdf::fonts::FontFamily {
        regular: load("DejaVuSans.ttf")?,
        bold: load("DejaVuSans-Bold.ttf")?,
        italic: load("DejaVuSans-Oblique.ttf")?,
        bold_italic: load("DejaVuSans-BoldOblique.ttf")?,
    };
    let mut doc = genpdf::Document::new(family);
    doc.set_title(format!("IMIS-Station {} {}", st.code, st.label));
    doc.set_minimal_conformance();
    let mut deco = genpdf::SimplePageDecorator::new();
    deco.set_margins(16);
    doc.set_page_decorator(deco);

    let line = |doc: &mut genpdf::Document, text: &str, style: Style, align: Alignment| {
        let mut p = Paragraph::default();
        p.push_styled(text.to_string(), style);
        doc.push(p.aligned(align));
    };

    line(&mut doc, "SLF-MESSSTATION (IMIS)", Style::new().with_color(GOLD).with_font_size(10).bold(), Alignment::Center);
    doc.push(Break::new(0.3));
    line(&mut doc, &format!("{} {}", st.code, st.label), Style::new().with_color(INK).with_font_size(22).bold(), Alignment::Center);
    doc.push(Break::new(0.2));
    let (e, n) = wgs84_to_lv95(st.lat, st.lon);
    line(
        &mut doc,
        &format!("{} ({}) · {:.0} m ü.M. · {}", region(&st.code), st.canton, st.elevation, st.kind),
        Style::new().with_color(ACCENT).with_font_size(11).italic(),
        Alignment::Center,
    );
    line(
        &mut doc,
        &format!("WGS84 {:.5}° N, {:.5}° E · LV95 {:.0} / {:.0}", st.lat, st.lon, e, n),
        Style::new().with_color(GREY).with_font_size(9),
        Alignment::Center,
    );
    doc.push(Break::new(0.5));

    for (path, w, _h) in maps {
        let dpi = *w as f64 * 25.4 / 170.0;
        doc.push(
            Image::from_path(path)
                .map_err(|e| anyhow!("Kartenbild laden: {e}"))?
                .with_alignment(Alignment::Center)
                .with_dpi(dpi),
        );
        doc.push(Break::new(0.35));
    }

    line(
        &mut doc,
        &format!("Messwerte · Stand {measured_at}"),
        Style::new().with_color(ACCENT).with_font_size(13).bold(),
        Alignment::Left,
    );
    doc.push(Break::new(0.2));
    // Zwei Werte pro Zeile, damit alles auf eine Seite passt.
    let pairs = |doc: &mut genpdf::Document, values: &[(String, String)]| {
        for pair in values.chunks(2) {
            let mut p = Paragraph::default();
            for (i, (k, v)) in pair.iter().enumerate() {
                if i > 0 {
                    p.push_styled("     ".to_string(), Style::new().with_font_size(10));
                }
                p.push_styled(format!("{k}: "), Style::new().with_color(GREY).with_font_size(10));
                p.push_styled(v.clone(), Style::new().with_color(INK).with_font_size(10).bold());
            }
            doc.push(p);
        }
    };
    pairs(&mut doc, values);
    for (nb, at, vals) in neighbours {
        doc.push(Break::new(0.4));
        let (e, n) = wgs84_to_lv95(nb.lat, nb.lon);
        line(
            &mut doc,
            &format!("{} {} ({}) · {:.0} m ü.M. · {} · Stand {at}", nb.code, nb.label, region(&nb.code), nb.elevation, nb.kind),
            Style::new().with_color(Color::Rgb(0x1f, 0x5f, 0xbf)).with_font_size(12).bold(),
            Alignment::Left,
        );
        line(
            &mut doc,
            &format!("WGS84 {:.5}° N, {:.5}° E · LV95 {:.0} / {:.0}", nb.lat, nb.lon, e, n),
            Style::new().with_color(GREY).with_font_size(9),
            Alignment::Left,
        );
        pairs(&mut doc, vals);
    }
    doc.push(Break::new(0.5));
    line(
        &mut doc,
        "Roter Punkt = Hauptstation, blaue Punkte = weitere IMIS-Stationen im Ausschnitt. Karte: © swisstopo (Landeskarte, Höhenkurven). \
Messdaten: WSL-Institut für Schnee- und Lawinenforschung SLF, IMIS-Messnetz (measurement-api.slf.ch).",
        Style::new().with_color(GREY).with_font_size(8).italic(),
        Alignment::Left,
    );

    std::fs::create_dir_all(out.parent().unwrap_or(Path::new(".")))?;
    doc.render_to_file(out).map_err(|e| anyhow!("render {}: {e}", out.display()))?;
    Ok(())
}

// ---- main -----------------------------------------------------------------

fn main() -> Result<()> {
    let mut code = "OFE2".to_string();
    let mut out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--out" => out = args.next(),
            _ if a.starts_with("--") => return Err(anyhow!("unbekannte Option {a}")),
            _ => code = a.to_uppercase(),
        }
    }

    let rt = tokio::runtime::Runtime::new()?;
    let client = reqwest::Client::builder()
        .user_agent("pegelstand-imisstation/1.0 (+https://github.com/zdavatz/pegelstand)")
        .build()?;

    let stations: Vec<serde_json::Value> =
        rt.block_on(async { client.get(format!("{API}/stations")).send().await?.error_for_status()?.json().await })?;
    let to_station = |m: &serde_json::Value| -> Option<Station> {
        Some(Station {
            code: m["code"].as_str()?.to_string(),
            label: m["label"].as_str().unwrap_or("").to_string(),
            lat: m["lat"].as_f64()?,
            lon: m["lon"].as_f64()?,
            elevation: m["elevation"].as_f64().unwrap_or(0.0),
            kind: match m["type"].as_str().unwrap_or("") {
                "SNOW_FLAT" => "Schneestation (Flachfeld)".to_string(),
                "WIND" => "Windstation".to_string(),
                other => other.to_string(),
            },
            canton: m["canton_code"].as_str().unwrap_or("").to_string(),
        })
    };
    let all: Vec<Station> = stations.iter().filter_map(to_station).collect();
    let st = all.iter().find(|s| s.code == code).ok_or_else(|| anyhow!("Unbekannte IMIS-Station: {code}"))?;
    // Nachbarstationen: alle IMIS-Stationen im Umkreis von 6 km.
    let near: Vec<&Station> = all
        .iter()
        .filter(|s| s.code != st.code)
        .filter(|s| {
            let dy = (s.lat - st.lat) * 111_320.0;
            let dx = (s.lon - st.lon) * 111_320.0 * st.lat.to_radians().cos();
            (dx * dx + dy * dy).sqrt() < 6_000.0
        })
        .collect();
    let rows: Vec<serde_json::Value> = rt.block_on(async {
        client.get(format!("{API}/station/{code}/measurements")).send().await?.error_for_status()?.json().await
    })?;
    let (measured_at, values) = summary(&rows);
    let mut neighbours = Vec::new();
    for nb in &near {
        let r: Vec<serde_json::Value> = rt.block_on(async {
            client.get(format!("{API}/station/{}/measurements", nb.code)).send().await?.error_for_status()?.json().await
        })?;
        let (at, vals) = summary(&r);
        println!("  Nachbarstation: {} {}", nb.code, nb.label);
        neighbours.push((*nb, at, vals));
    }

    // Übersicht zentriert auf den Schwerpunkt aller Stationen.
    let mut marks: Vec<&Station> = vec![st];
    marks.extend(near.iter().copied());
    let mid = (
        marks.iter().map(|s| s.lat).sum::<f64>() / marks.len() as f64,
        marks.iter().map(|s| s.lon).sum::<f64>() / marks.len() as f64,
    );

    std::fs::create_dir_all(OUT_DIR)?;
    let mut maps = Vec::new();
    for (zoom, center, caption, name) in [
        (15u32, (st.lat, st.lon), "Detail", "detail"),
        (13u32, mid, "Übersicht Ofenpass – Buffalora / Val Müstair", "uebersicht"),
    ] {
        let caption = if name == "detail" { format!("Detail {} {}", st.code, st.label) } else { caption.to_string() };
        let (rgba, w, h) = rt.block_on(render_map(&client, center, &marks, zoom, &caption))?;
        let path = PathBuf::from(OUT_DIR).join(format!("{}_{name}.png", code.to_lowercase()));
        save_rgb(rgba, w, h, &path)?;
        println!("  Karte: {} ({}×{} px)", path.display(), w, h);
        maps.push((path, w, h));
    }

    let ascii = |s: &str| s.replace('ö', "oe").replace('ü', "ue").replace('ä', "ae").replace(' ', "_");
    let out_path = PathBuf::from(out.unwrap_or_else(|| format!("{OUT_DIR}/IMIS_{}_{}.pdf", code, ascii(&st.label))));
    let font_dir = std::env::var("FONT_DIR").unwrap_or_else(|_| DEFAULT_FONT_DIR.into());
    build_pdf(&font_dir, st, &maps, &measured_at, &values, &neighbours, &out_path)?;
    println!("  PDF: {}", out_path.display());
    Ok(())
}
