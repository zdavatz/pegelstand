// Bojendistanzmessung — PDF-Report aus einem oder mehreren u-blox-GPS-Logs (CSV).
//
// Zeichnet einen oder mehrere GPS-Tracks über eine Karte (Google-Satellit via
// Maps Static API, oder OpenStreetMap-Kacheln), clustert die Track-Endpunkte zu
// Bojen und erzeugt ein PDF mit Karte + Kennzahlen je Messung (Distanz A→B,
// Höchst-/Tiefstgeschwindigkeit, Zeit/Dauer) sowie der Gesamt-Bojenlinie.
//
//   cargo run --release --bin bojendistanz -- a.csv b.csv c.csv
//   cargo run --release --bin bojendistanz -- a.csv --osm --title "Seebad Zollikon"
//
// Google-Basemap braucht einen Maps-Static-Key in $GOOGLE_MAPS_STATIC_KEY oder
// ~/.config/pegelstand/maps-static-key.txt. Ohne Key: --osm verwenden.

use std::f64::consts::PI;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use base64::Engine;
use genpdf::elements::{Break, Image, Paragraph};
use genpdf::style::{Color, Style};
use genpdf::Alignment;

const DEFAULT_FONT_DIR: &str = "/usr/share/fonts/dejavu";
const OUT_DIR: &str = "messung";
const EARTH_R: f64 = 6_371_000.0; // m
const TILE: f64 = 256.0;

// Standard-Messungen (falls keine CSVs als Argument übergeben werden).
const DEFAULT_CSVS: &[&str] = &[
    "messung/UbloxGps_20260706_142025.csv",
    "messung/zo_2.csv",
    "messung/zo_3.csv",
];

// Palette (Text/PDF).
const INK: Color = Color::Rgb(0x1a, 0x1a, 0x1a);
const ACCENT: Color = Color::Rgb(0x0d, 0x47, 0x6b);
const GOLD: Color = Color::Rgb(0x9a, 0x7b, 0x2e);
const GREY: Color = Color::Rgb(0x55, 0x55, 0x55);

// Track-Farben (hell, auf dunklem Satellitenbild gut sichtbar): (r,g,b).
const TRACK_COLORS: &[(u8, u8, u8)] = &[
    (0xff, 0x3b, 0x30), // rot
    (0x2e, 0x9b, 0xff), // blau
    (0x34, 0xdd, 0x5a), // grün
    (0xff, 0xa5, 0x1f), // orange
    (0xc9, 0x6b, 0xff), // violett
    (0x2a, 0xe0, 0xd0), // türkis
];

#[derive(Clone)]
struct Point {
    lat: f64,
    lon: f64,
    speed: Option<f64>,
    utc: Option<f64>,
}

struct Track {
    name: String,
    color: (u8, u8, u8),
    pts: Vec<Point>,
    dist_ab: f64,
    path_len: f64,
    v_min: f64,
    v_max: f64,
    utc_start: f64,
    utc_end: f64,
    a_buoy: usize,
    b_buoy: usize,
}

// ---- Geo ------------------------------------------------------------------

fn haversine(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_R * a.sqrt().asin()
}

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

// ---- CSV ------------------------------------------------------------------

fn parse_csv(path: &Path) -> Result<Vec<Point>> {
    let mut rdr = csv::ReaderBuilder::new().flexible(true).from_path(path)?;
    let mut pts = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        let get = |i: usize| rec.get(i).map(|s| s.trim()).filter(|s| !s.is_empty());
        let (lat, lon) = match (
            get(2).and_then(|s| s.parse::<f64>().ok()),
            get(3).and_then(|s| s.parse::<f64>().ok()),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        let utc = get(1).and_then(|s| s.parse::<f64>().ok()).map(|v| {
            let h = (v / 10000.0).floor();
            let m = ((v - h * 10000.0) / 100.0).floor();
            let s = v - h * 10000.0 - m * 100.0;
            h * 3600.0 + m * 60.0 + s
        });
        let speed = get(5).and_then(|s| s.parse::<f64>().ok());
        pts.push(Point { lat, lon, speed, utc });
    }
    if pts.len() < 2 {
        return Err(anyhow!("{}: zu wenige gültige GPS-Punkte", path.display()));
    }
    Ok(pts)
}

fn load_track(path: &Path, name: String, color: (u8, u8, u8)) -> Result<Track> {
    let pts = parse_csv(path)?;
    let a = &pts[0];
    let b = pts.last().unwrap();
    let dist_ab = haversine(a.lat, a.lon, b.lat, b.lon);
    let path_len: f64 = pts
        .windows(2)
        .map(|w| haversine(w[0].lat, w[0].lon, w[1].lat, w[1].lon))
        .sum();
    let sp: Vec<f64> = pts.iter().filter_map(|p| p.speed).collect();
    let v_min = sp.iter().cloned().fold(f64::MAX, f64::min);
    let v_max = sp.iter().cloned().fold(f64::MIN, f64::max);
    let ut: Vec<f64> = pts.iter().filter_map(|p| p.utc).collect();
    let utc_start = ut.iter().cloned().fold(f64::MAX, f64::min);
    let utc_end = ut.iter().cloned().fold(f64::MIN, f64::max);
    Ok(Track {
        name,
        color,
        pts,
        dist_ab,
        path_len,
        v_min,
        v_max,
        utc_start,
        utc_end,
        a_buoy: 0,
        b_buoy: 0,
    })
}

// ---- Bojen-Clustering (Track-Endpunkte) -----------------------------------

/// Clustert alle Track-Endpunkte (A und B) zu Bojen (Schwelle ~12 m), gibt die
/// gemittelten Bojenpositionen (nach Breitengrad Nord→Süd sortiert) zurück und
/// setzt a_buoy/b_buoy je Track auf den Bojen-Index.
fn cluster_buoys(tracks: &mut [Track]) -> Vec<(f64, f64)> {
    const THRESH: f64 = 14.0; // m
    let mut buoys: Vec<(f64, f64, usize)> = Vec::new(); // (lat_sum,lon_sum,count) → gemittelt
    // Endpunkte einsammeln, jeweils zu nächster Boje oder neuer Boje.
    let assign = |buoys: &mut Vec<(f64, f64, usize)>, lat: f64, lon: f64| -> usize {
        let mut best = None;
        for (i, b) in buoys.iter().enumerate() {
            let (blat, blon) = (b.0 / b.2 as f64, b.1 / b.2 as f64);
            let d = haversine(lat, lon, blat, blon);
            if d < THRESH && best.map_or(true, |(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        match best {
            Some((i, _)) => {
                buoys[i].0 += lat;
                buoys[i].1 += lon;
                buoys[i].2 += 1;
                i
            }
            None => {
                buoys.push((lat, lon, 1));
                buoys.len() - 1
            }
        }
    };
    // Rohzuordnung (Reihenfolge = Trackreihenfolge).
    let mut raw: Vec<(usize, usize)> = Vec::new();
    for t in tracks.iter() {
        let a = &t.pts[0];
        let b = t.pts.last().unwrap();
        let ia = assign(&mut buoys, a.lat, a.lon);
        let ib = assign(&mut buoys, b.lat, b.lon);
        raw.push((ia, ib));
    }
    // Gemittelte Positionen.
    let centers: Vec<(f64, f64)> = buoys
        .iter()
        .map(|b| (b.0 / b.2 as f64, b.1 / b.2 as f64))
        .collect();
    // Nach Breitengrad Nord→Süd sortieren; Remap alt→neu.
    let mut order: Vec<usize> = (0..centers.len()).collect();
    order.sort_by(|&i, &j| centers[j].0.partial_cmp(&centers[i].0).unwrap());
    let mut remap = vec![0usize; centers.len()];
    for (new_i, &old_i) in order.iter().enumerate() {
        remap[old_i] = new_i;
    }
    let sorted: Vec<(f64, f64)> = order.iter().map(|&i| centers[i]).collect();
    for (t, (ia, ib)) in tracks.iter_mut().zip(raw) {
        t.a_buoy = remap[ia];
        t.b_buoy = remap[ib];
    }
    sorted
}

// ---- Google Static Maps ---------------------------------------------------

fn read_maps_key() -> Option<String> {
    if let Ok(k) = std::env::var("GOOGLE_MAPS_STATIC_KEY") {
        let k = k.trim().to_string();
        if !k.is_empty() {
            return Some(k);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let p = Path::new(&home).join(".config/pegelstand/maps-static-key.txt");
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn fetch_google_static(
    client: &reqwest::Client,
    clat: f64,
    clon: f64,
    zoom: u32,
    size: u32,
    scale: u32,
    key: &str,
) -> Result<Vec<u8>> {
    let url = format!(
        "https://maps.googleapis.com/maps/api/staticmap?center={clat:.6},{clon:.6}\
         &zoom={zoom}&size={size}x{size}&scale={scale}&maptype=satellite&format=png&key={key}"
    );
    let resp = client.get(&url).send().await?;
    let status = resp.status();
    let bytes = resp.bytes().await?;
    if !status.is_success() {
        return Err(anyhow!(
            "Google Static Maps HTTP {}: {}",
            status,
            String::from_utf8_lossy(&bytes).chars().take(200).collect::<String>()
        ));
    }
    Ok(bytes.to_vec())
}

// ---- OSM-Kacheln ----------------------------------------------------------

async fn fetch_tile(client: &reqwest::Client, z: u32, x: i64, y: i64) -> Option<Vec<u8>> {
    let url = format!("https://tile.openstreetmap.org/{z}/{x}/{y}.png");
    let resp = client
        .get(&url)
        .header(
            reqwest::header::USER_AGENT,
            "pegelstand-bojendistanz/1.0 (+https://github.com/zdavatz/pegelstand; zdavatz@gmail.com)",
        )
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.bytes().await.ok().map(|b| b.to_vec())
}

// ---- Overlay-SVG (gemeinsam für beide Basemaps) ---------------------------

/// Baut das vollständige Karten-SVG: Hintergrund (`bg`), Tracks, Bojen,
/// Massstab, Legende. `project` bildet (lon,lat) → SVG-Canvas-Koordinaten ab.
fn overlay_svg(
    w: f64,
    h: f64,
    bg: &str,
    tracks: &[Track],
    buoys: &[(f64, f64)],
    project: &dyn Fn(f64, f64) -> (f64, f64),
    mpp: f64,
    place: &str,
) -> String {
    let hexf = |c: (u8, u8, u8)| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
    let mut s = String::new();
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">\n\
         <rect width=\"{w}\" height=\"{h}\" fill=\"#0b1a24\"/>\n{bg}\n"
    ));

    // Tracks: weisses Casing + Farblinie.
    for t in tracks {
        let poly: String = t
            .pts
            .iter()
            .map(|p| {
                let (x, y) = project(p.lon, p.lat);
                format!("{x:.1},{y:.1}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        s.push_str(&format!(
            "<polyline points=\"{poly}\" fill=\"none\" stroke=\"#ffffff\" stroke-width=\"6\" \
             stroke-opacity=\"0.55\" stroke-linejoin=\"round\" stroke-linecap=\"round\"/>\n\
             <polyline points=\"{poly}\" fill=\"none\" stroke=\"{col}\" stroke-width=\"3\" \
             stroke-linejoin=\"round\" stroke-linecap=\"round\"/>\n",
            col = hexf(t.color),
        ));
    }

    // Bojen: weisser Ring + Nummer.
    for (i, (blat, blon)) in buoys.iter().enumerate() {
        let (x, y) = project(*blon, *blat);
        s.push_str(&format!(
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"9\" fill=\"#ffd21f\" stroke=\"#1a1a1a\" stroke-width=\"2.5\"/>\n\
             <text x=\"{x:.1}\" y=\"{ty:.1}\" font-family=\"DejaVu Sans, sans-serif\" font-size=\"13\" \
             font-weight=\"bold\" fill=\"#1a1a1a\" text-anchor=\"middle\">{n}</text>\n",
            ty = y + 4.6,
            n = i + 1,
        ));
    }

    // Massstab (bottom-left, oberhalb der Google-Signatur).
    let nice = [5.0, 10.0, 20.0, 50.0, 100.0, 200.0];
    let bar_m = *nice.iter().rev().find(|&&m| m / mpp <= w * 0.3).unwrap_or(&10.0);
    let bar_px = bar_m / mpp;
    let bx0 = 16.0;
    let by0 = h - 48.0;
    s.push_str(&format!(
        "<rect x=\"{rx:.1}\" y=\"{ry:.1}\" width=\"{rw:.1}\" height=\"24\" rx=\"3\" fill=\"#000000\" fill-opacity=\"0.5\"/>\n\
         <line x1=\"{bx0:.1}\" y1=\"{by0:.1}\" x2=\"{bx1:.1}\" y2=\"{by0:.1}\" stroke=\"#ffffff\" stroke-width=\"3\"/>\n\
         <line x1=\"{bx0:.1}\" y1=\"{t0:.1}\" x2=\"{bx0:.1}\" y2=\"{t1:.1}\" stroke=\"#ffffff\" stroke-width=\"3\"/>\n\
         <line x1=\"{bx1:.1}\" y1=\"{t0:.1}\" x2=\"{bx1:.1}\" y2=\"{t1:.1}\" stroke=\"#ffffff\" stroke-width=\"3\"/>\n\
         <text x=\"{tx:.1}\" y=\"{tyt:.1}\" font-family=\"DejaVu Sans, sans-serif\" font-size=\"12\" fill=\"#ffffff\">{bm:.0} m</text>\n",
        rx = bx0 - 8.0, ry = by0 - 16.0, rw = bar_px + 54.0,
        bx0 = bx0, by0 = by0, bx1 = bx0 + bar_px,
        t0 = by0 - 5.0, t1 = by0 + 5.0,
        tx = bx0 + bar_px + 8.0, tyt = by0 + 4.0, bm = bar_m,
    ));

    // Legende oben links.
    let lx = 14.0;
    let mut ly = 22.0;
    let rows = tracks.len() as f64 + 1.0;
    s.push_str(&format!(
        "<rect x=\"6\" y=\"6\" width=\"262\" height=\"{lh:.0}\" rx=\"4\" fill=\"#000000\" fill-opacity=\"0.5\"/>\n\
         <text x=\"{lx:.1}\" y=\"{ly:.1}\" font-family=\"DejaVu Sans, sans-serif\" font-size=\"12\" font-weight=\"bold\" fill=\"#ffffff\">{place}</text>\n",
        lh = rows * 18.0 + 8.0,
    ));
    for t in tracks {
        ly += 18.0;
        s.push_str(&format!(
            "<line x1=\"{lx:.1}\" y1=\"{yl:.1}\" x2=\"{lx2:.1}\" y2=\"{yl:.1}\" stroke=\"{col}\" stroke-width=\"4\"/>\n\
             <text x=\"{tx:.1}\" y=\"{yt:.1}\" font-family=\"DejaVu Sans, sans-serif\" font-size=\"11\" fill=\"#ffffff\">{nm}: Boje {a}\u{2013}{b}, {d:.1} m</text>\n",
            yl = ly - 4.0, lx2 = lx + 22.0, col = hexf(t.color),
            tx = lx + 28.0, yt = ly,
            nm = t.name, a = t.a_buoy + 1, b = t.b_buoy + 1, d = t.dist_ab,
        ));
    }

    s.push_str("</svg>\n");
    s
}

fn svg_to_rgba(svg: &str, out_w: u32, out_h: u32, render_scale: f32) -> Result<Vec<u8>> {
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &opt)
        .map_err(|e| anyhow!("SVG parse: {e}"))?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(out_w, out_h)
        .ok_or_else(|| anyhow!("Pixmap fehlgeschlagen"))?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE);
    let transform = resvg::tiny_skia::Transform::from_scale(render_scale, render_scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(pixmap.data().to_vec())
}

/// Google-Satellit als Basemap. Gibt (rgba, w_px, h_px, note).
async fn render_google(
    tracks: &[Track],
    buoys: &[(f64, f64)],
    place: &str,
    key: &str,
) -> Result<(Vec<u8>, u32, u32, String)> {
    // Kombinierte BBox aller Punkte.
    let (mut mnlat, mut mxlat, mut mnlon, mut mxlon) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for t in tracks {
        for p in &t.pts {
            mnlat = mnlat.min(p.lat);
            mxlat = mxlat.max(p.lat);
            mnlon = mnlon.min(p.lon);
            mxlon = mxlon.max(p.lon);
        }
    }
    let clat = (mnlat + mxlat) / 2.0;
    let clon = (mnlon + mxlon) / 2.0;
    let size = 640u32;
    let scale = 2u32;
    // Höchster Zoom, bei dem die BBox (mit ~30 % Rand) ins Bild passt.
    let fits = |z: u32| {
        let (x0, y0) = lonlat_to_px(mnlon, mxlat, z);
        let (x1, y1) = lonlat_to_px(mxlon, mnlat, z);
        (x1 - x0).abs() <= size as f64 * 0.70 && (y1 - y0).abs() <= size as f64 * 0.70
    };
    let zoom = (14..=20).rev().find(|&z| fits(z)).unwrap_or(18);

    let client = reqwest::Client::new();
    let img = fetch_google_static(&client, clat, clon, zoom, size, scale, key).await?;
    let engine = base64::engine::general_purpose::STANDARD;
    let b64 = engine.encode(&img);
    let iw = (size * scale) as f64;

    // Projektion: (lon,lat) → Bildkoordinaten (im scale-fachen Bild).
    let (cgx, cgy) = lonlat_to_px(clon, clat, zoom);
    let project = move |lon: f64, lat: f64| {
        let (gx, gy) = lonlat_to_px(lon, lat, zoom);
        ((gx - cgx + size as f64 / 2.0) * scale as f64, (gy - cgy + size as f64 / 2.0) * scale as f64)
    };
    let mpp = meters_per_px(clat, zoom) / scale as f64;
    let bg = format!(
        "<image x=\"0\" y=\"0\" width=\"{iw}\" height=\"{iw}\" href=\"data:image/png;base64,{b64}\"/>"
    );
    let svg = overlay_svg(iw, iw, &bg, tracks, buoys, &project, mpp, place);
    let rgba = svg_to_rgba(&svg, iw as u32, iw as u32, 1.0)?;
    Ok((rgba, iw as u32, iw as u32, format!("Satellitenbild © Google, Zoom {zoom}")))
}

/// OSM-Kacheln als Basemap (Fallback).
async fn render_osm(
    tracks: &[Track],
    buoys: &[(f64, f64)],
    place: &str,
) -> Result<(Vec<u8>, u32, u32, String)> {
    let all: Vec<&Point> = tracks.iter().flat_map(|t| t.pts.iter()).collect();
    let lat0 = all.iter().map(|p| p.lat).sum::<f64>() / all.len() as f64;
    let bbox = |z: u32| {
        let (mut a, mut b, mut c, mut d) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in &all {
            let (x, y) = lonlat_to_px(p.lon, p.lat, z);
            a = a.min(x);
            b = b.min(y);
            c = c.max(x);
            d = d.max(y);
        }
        (a, b, c, d)
    };
    let zoom = (10..=19)
        .rev()
        .find(|&z| {
            let (a, b, c, d) = bbox(z);
            (c - a) <= 620.0 && (d - b) <= 620.0
        })
        .unwrap_or(19);
    let (mnx, mny, mxx, mxy) = bbox(zoom);
    let pad = ((mxx - mnx).max(mxy - mny) * 0.35).max(120.0);
    let cx0 = mnx - pad;
    let cy0 = mny - pad;
    let cx1 = mxx + pad;
    let cy1 = mxy + pad;
    let w = (cx1 - cx0).round();
    let h = (cy1 - cy0).round();

    let n = 2i64.pow(zoom);
    let client = reqwest::Client::new();
    let engine = base64::engine::general_purpose::STANDARD;
    let mut bg = String::new();
    for tx in (cx0 / TILE).floor() as i64..=(cx1 / TILE).floor() as i64 {
        for ty in (cy0 / TILE).floor() as i64..=(cy1 / TILE).floor() as i64 {
            if tx < 0 || ty < 0 || tx >= n || ty >= n {
                continue;
            }
            if let Some(bytes) = fetch_tile(&client, zoom, tx, ty).await {
                let b64 = engine.encode(&bytes);
                bg.push_str(&format!(
                    "<image x=\"{px:.1}\" y=\"{py:.1}\" width=\"256\" height=\"256\" href=\"data:image/png;base64,{b64}\"/>\n",
                    px = tx as f64 * TILE - cx0,
                    py = ty as f64 * TILE - cy0,
                ));
            }
        }
    }
    let project = move |lon: f64, lat: f64| {
        let (x, y) = lonlat_to_px(lon, lat, zoom);
        (x - cx0, y - cy0)
    };
    let mpp = meters_per_px(lat0, zoom);
    let svg = overlay_svg(w, h, &bg, tracks, buoys, &project, mpp, place);
    let rgba = svg_to_rgba(&svg, (w * 2.0) as u32, (h * 2.0) as u32, 2.0)?;
    Ok((rgba, (w * 2.0) as u32, (h * 2.0) as u32, "Kartendaten © OpenStreetMap-Mitwirkende".into()))
}

// ---- Zeit -----------------------------------------------------------------

fn fmt_hms(secs: f64) -> String {
    let s = secs.round() as i64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}
fn fmt_dur(secs: f64) -> String {
    let s = secs.round() as i64;
    if s >= 60 {
        format!("{} min {} s", s / 60, s % 60)
    } else {
        format!("{} s", s)
    }
}
fn parse_name_datetime(path: &Path) -> Option<(String, String)> {
    let stem = path.file_stem()?.to_str()?;
    let parts: Vec<&str> = stem.split('_').collect();
    let d = parts.iter().find(|p| p.len() == 8 && p.chars().all(|c| c.is_ascii_digit()))?;
    let t = parts.iter().find(|p| p.len() == 6 && p.chars().all(|c| c.is_ascii_digit()))?;
    Some((
        format!("{}.{}.{}", &d[6..8], &d[4..6], &d[0..4]),
        format!("{}:{}:{}", &t[0..2], &t[2..4], &t[4..6]),
    ))
}

// ---- PDF ------------------------------------------------------------------

fn build_pdf(
    font_dir: &str,
    map_png: &Path,
    map_w: u32,
    map_h: u32,
    place: &str,
    date: &str,
    tracks: &[Track],
    buoys: &[(f64, f64)],
    attribution: &str,
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
    doc.set_title(format!("Bojendistanzmessung — {place}"));
    doc.set_minimal_conformance();
    let mut deco = genpdf::SimplePageDecorator::new();
    deco.set_margins(20);
    doc.set_page_decorator(deco);

    let line = |doc: &mut genpdf::Document, text: &str, style: Style, align: Alignment| {
        let mut p = Paragraph::default();
        p.push_styled(text.to_string(), style);
        doc.push(p.aligned(align));
    };

    line(&mut doc, "BOJENDISTANZMESSUNG", Style::new().with_color(GOLD).with_font_size(11).bold(), Alignment::Center);
    doc.push(Break::new(0.5));
    line(&mut doc, place, Style::new().with_color(INK).with_font_size(22).bold(), Alignment::Center);
    doc.push(Break::new(0.4));
    line(
        &mut doc,
        &format!("GPS-Vermessung mit u-blox-Empfänger · {} Messungen · {}", tracks.len(), date),
        Style::new().with_color(ACCENT).with_font_size(12).italic(),
        Alignment::Center,
    );
    doc.push(Break::new(0.7));

    // Karte auf ~170 mm Breite / ~132 mm Höhe.
    let dpi = (map_w as f64 * 25.4 / 170.0).max(map_h as f64 * 25.4 / 132.0);
    let img = Image::from_path(map_png)
        .map_err(|e| anyhow!("Kartenbild laden: {e}"))?
        .with_alignment(Alignment::Center)
        .with_dpi(dpi);
    doc.push(img);
    doc.push(Break::new(0.25));
    line(
        &mut doc,
        "Gelbe Punkte = Bojen (Nord→Süd nummeriert) · farbige Pfade = GPS-Tracks je Messung",
        Style::new().with_color(GREY).with_font_size(8).italic(),
        Alignment::Center,
    );
    doc.push(Break::new(0.6));

    line(&mut doc, "Ergebnisse je Messung", Style::new().with_color(ACCENT).with_font_size(14).bold(), Alignment::Left);
    doc.push(Break::new(0.3));

    for t in tracks {
        let mut p = Paragraph::default();
        p.push_styled("● ".to_string(), Style::new().with_color(Color::Rgb(t.color.0, t.color.1, t.color.2)).with_font_size(11).bold());
        p.push_styled(
            format!("{} (Boje {}\u{2013}{}, {}):  ", t.name, t.a_buoy + 1, t.b_buoy + 1, fmt_hms(t.utc_start + 7200.0)),
            Style::new().with_color(INK).with_font_size(11).bold(),
        );
        p.push_styled(
            format!(
                "A→B {:.1} m · Weg {:.1} m · v {:.2}\u{2013}{:.2} km/h · {}",
                t.dist_ab, t.path_len, t.v_min, t.v_max, fmt_dur(t.utc_end - t.utc_start)
            ),
            Style::new().with_color(GREY).with_font_size(11),
        );
        doc.push(p);
        doc.push(Break::new(0.12));
    }

    // Bojenlinie: aufeinanderfolgende Bojen (Nord→Süd) und Gesamtlänge.
    doc.push(Break::new(0.35));
    let mut seg = String::new();
    let mut total = 0.0;
    for i in 0..buoys.len().saturating_sub(1) {
        let d = haversine(buoys[i].0, buoys[i].1, buoys[i + 1].0, buoys[i + 1].1);
        total += d;
        if !seg.is_empty() {
            seg.push_str(" + ");
        }
        seg.push_str(&format!("Boje {}\u{2013}{} {:.1} m", i + 1, i + 2, d));
    }
    let mut p = Paragraph::default();
    p.push_styled(
        format!("Bojenlinie ({} Bojen):  ", buoys.len()),
        Style::new().with_color(ACCENT).with_font_size(12).bold(),
    );
    p.push_styled(
        format!("{seg}  =  {total:.1} m gesamt"),
        Style::new().with_color(INK).with_font_size(12).bold(),
    );
    doc.push(p);

    doc.push(Break::new(0.6));
    line(
        &mut doc,
        &format!(
            "Distanz A→B je Messung = Grosskreis (Haversine) zwischen erstem und letztem GPS-Fix. \
Bojenpositionen aus den geclusterten Track-Endpunkten gemittelt. {attribution}."
        ),
        Style::new().with_color(GREY).with_font_size(8).italic(),
        Alignment::Left,
    );

    std::fs::create_dir_all(out.parent().unwrap_or(Path::new(".")))?;
    doc.render_to_file(out).map_err(|e| anyhow!("render {}: {e}", out.display()))?;
    Ok(())
}

// ---- main -----------------------------------------------------------------

fn main() -> Result<()> {
    let mut place = "Seebad Zollikon".to_string();
    let mut out: Option<String> = None;
    let mut force_osm = false;
    let mut csvs: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--title" | "--place" => place = args.next().unwrap_or(place),
            "--out" => out = args.next(),
            "--osm" => force_osm = true,
            _ if a.starts_with("--") => return Err(anyhow!("unbekannte Option {a}")),
            _ => csvs.push(a),
        }
    }
    if csvs.is_empty() {
        csvs = DEFAULT_CSVS.iter().map(|s| s.to_string()).collect();
    }

    // Tracks laden (chronologisch nach Startzeit sortieren → stabile Nummerierung).
    let mut tracks: Vec<Track> = Vec::new();
    for (i, c) in csvs.iter().enumerate() {
        let color = TRACK_COLORS[i % TRACK_COLORS.len()];
        tracks.push(load_track(Path::new(c), format!("Messung {}", i + 1), color)?);
    }
    tracks.sort_by(|a, b| a.utc_start.partial_cmp(&b.utc_start).unwrap());
    for (i, t) in tracks.iter_mut().enumerate() {
        t.name = format!("Messung {}", i + 1);
        t.color = TRACK_COLORS[i % TRACK_COLORS.len()];
    }

    let buoys = cluster_buoys(&mut tracks);

    let date = csvs
        .iter()
        .find_map(|c| parse_name_datetime(Path::new(c)).map(|(d, _)| d))
        .unwrap_or_else(|| chrono::Local::now().format("%d.%m.%Y").to_string());

    println!("  Messungen: {}", tracks.len());
    for t in &tracks {
        println!(
            "    {} Boje{}→{}: A→B {:.1} m, v {:.2}–{:.2} km/h, {} s",
            t.name, t.a_buoy + 1, t.b_buoy + 1, t.dist_ab, t.v_min, t.v_max,
            (t.utc_end - t.utc_start).round() as i64
        );
    }
    println!("  Bojen: {}", buoys.len());

    let rt = tokio::runtime::Runtime::new()?;
    let key = read_maps_key();
    let (rgba, mw, mh, attrib) = if force_osm || key.is_none() {
        if key.is_none() && !force_osm {
            eprintln!("  Hinweis: kein Maps-Key gefunden → OSM-Basemap.");
        }
        rt.block_on(render_osm(&tracks, &buoys, &place))?
    } else {
        match rt.block_on(render_google(&tracks, &buoys, &place, key.as_ref().unwrap())) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("  Google-Basemap fehlgeschlagen ({e}) → OSM.");
                rt.block_on(render_osm(&tracks, &buoys, &place))?
            }
        }
    };

    std::fs::create_dir_all(OUT_DIR)?;
    let map_path = PathBuf::from(OUT_DIR).join("bojendistanz_map.png");
    let rgba_img = image::RgbaImage::from_raw(mw, mh, rgba)
        .ok_or_else(|| anyhow!("RGBA-Puffergrösse passt nicht"))?;
    image::DynamicImage::ImageRgba8(rgba_img).into_rgb8().save(&map_path)?;
    println!("  Karte: {} ({}×{} px) — {}", map_path.display(), mw, mh, attrib);

    let out_path = PathBuf::from(out.unwrap_or_else(|| {
        format!("{OUT_DIR}/Bojendistanz_{}.pdf", place.replace(' ', "_"))
    }));
    let font_dir = std::env::var("FONT_DIR").unwrap_or_else(|_| DEFAULT_FONT_DIR.into());
    build_pdf(&font_dir, &map_path, mw, mh, &place, &date, &tracks, &buoys, &attrib, &out_path)?;
    println!("  PDF: {}", out_path.display());
    Ok(())
}
