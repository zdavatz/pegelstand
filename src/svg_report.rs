use std::io::Write;

const HASH: &str = "#";

fn hc(hex: &str) -> String {
    format!("{}{}", HASH, hex)
}

fn svg_polyline(
    data: &[(f64, f64)],
    _w: f64, _h: f64, ml: f64, mt: f64, pw: f64, ph: f64,
    y_min: f64, y_max: f64,
    color: &str, fill: bool,
) -> String {
    let range = y_max - y_min;
    if data.is_empty() || range.abs() < 1e-9 { return String::new(); }

    let mut pts = String::new();
    for (xf, yv) in data {
        if yv.is_nan() { continue; }
        let x = ml + xf * pw;
        let y = mt + ph - ((yv - y_min) / range) * ph;
        if pts.is_empty() { pts.push_str(&format!("M{:.1},{:.1}", x, y)); }
        else { pts.push_str(&format!(" L{:.1},{:.1}", x, y)); }
    }
    let mut s = String::new();
    if fill && !pts.is_empty() {
        let fx = ml + data.iter().find(|(_, y)| !y.is_nan()).map(|(x, _)| *x).unwrap_or(0.0) * pw;
        let lx = ml + data.iter().rev().find(|(_, y)| !y.is_nan()).map(|(x, _)| *x).unwrap_or(1.0) * pw;
        let by = mt + ph;
        s.push_str(&format!(
            "<path d=\"{} L{:.1},{:.1} L{:.1},{:.1} Z\" fill=\"{}\" opacity=\"0.15\" stroke=\"none\"/>",
            pts, lx, by, fx, by, color
        ));
    }
    s.push_str(&format!(
        "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1.5\"/>",
        pts, color
    ));
    s
}

fn svg_dots(
    data: &[(f64, f64)],
    _w: f64, _h: f64, ml: f64, mt: f64, pw: f64, ph: f64,
    y_min: f64, y_max: f64, color: &str,
) -> String {
    let range = y_max - y_min;
    if data.is_empty() || range.abs() < 1e-9 { return String::new(); }
    let mut s = String::new();
    for (xf, yv) in data {
        if yv.is_nan() { continue; }
        let x = ml + xf * pw;
        let y = mt + ph - ((yv - y_min) / range) * ph;
        s.push_str(&format!("<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"1.5\" fill=\"{}\"/>", x, y, color));
    }
    s
}

fn svg_axes(
    _w: f64, _h: f64, ml: f64, mt: f64, pw: f64, ph: f64,
    y_min: f64, y_max: f64, y_unit: &str,
    x_labels: &[(f64, String)], y_steps: usize,
) -> String {
    let gray = hc("dee2e6");
    let muted = hc("6c757d");
    let mut s = String::new();
    for i in 0..=y_steps {
        let frac = i as f64 / y_steps as f64;
        let y = mt + ph - frac * ph;
        let val = y_min + frac * (y_max - y_min);
        s.push_str(&format!(
            "<line x1=\"{:.0}\" y1=\"{:.1}\" x2=\"{:.0}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"0.5\"/>",
            ml, y, ml + pw, y, gray
        ));
        let label = if (y_max - y_min) >= 10.0 { format!("{:.0}", val) } else { format!("{:.1}", val) };
        let suffix = if i == y_steps { format!(" {}", y_unit) } else { String::new() };
        s.push_str(&format!(
            "<text x=\"{:.0}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"10\" fill=\"{}\">{}{}</text>",
            ml - 4.0, y + 3.5, muted, label, suffix
        ));
    }
    let label_count = x_labels.len();
    for (idx, (xf, label)) in x_labels.iter().enumerate() {
        let x = ml + xf * pw;
        let anchor = if idx == 0 { "start" } else if idx == label_count - 1 { "end" } else { "middle" };
        s.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.0}\" text-anchor=\"{}\" font-size=\"9\" fill=\"{}\">{}</text>",
            x, mt + ph + 15.0, anchor, muted, label
        ));
    }
    s
}

fn extract_col(json_rows: &[String], col: usize) -> Vec<(f64, f64)> {
    let n = json_rows.len();
    json_rows.iter().enumerate().map(|(i, row)| {
        let xf = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
        let val = parse_col(row, col);
        (xf, val)
    }).collect()
}

fn parse_col(row: &str, col: usize) -> f64 {
    let inner = row.trim_start_matches('[').trim_end_matches(']');
    let mut parts: Vec<&str> = Vec::new();
    let mut in_q = false;
    let mut s = 0;
    for (i, c) in inner.char_indices() {
        if c == '"' { in_q = !in_q; }
        if c == ',' && !in_q { parts.push(&inner[s..i]); s = i + 1; }
    }
    parts.push(&inner[s..]);
    parts.get(col)
        .map(|s| s.trim().trim_matches('"'))
        .and_then(|s| if s == "null" { None } else { s.parse().ok() })
        .unwrap_or(f64::NAN)
}

fn min_max(data: &[(f64, f64)]) -> (f64, f64) {
    let vals: Vec<f64> = data.iter().map(|(_, v)| *v).filter(|v| !v.is_nan()).collect();
    if vals.is_empty() { return (0.0, 1.0); }
    let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if (max - min).abs() < 1e-9 { (min - 1.0, max + 1.0) } else { (min, max) }
}

fn nice_range(min: f64, max: f64) -> (f64, f64) {
    let pad = (max - min) * 0.1;
    let lo = (min - pad).floor();
    let hi = (max + pad).ceil();
    if (hi - lo).abs() < 1e-9 { (lo - 1.0, hi + 1.0) } else { (lo, hi) }
}

fn combined_range(datasets: &[&[(f64, f64)]]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for d in datasets {
        let (a, b) = min_max(d);
        if a < lo { lo = a; }
        if b > hi { hi = b; }
    }
    nice_range(lo, hi)
}

pub fn make_x_labels(json_rows: &[String], max_labels: usize) -> Vec<(f64, String)> {
    let n = json_rows.len();
    if n == 0 { return vec![]; }
    let step = std::cmp::max(1, n / max_labels);
    let mut labels = Vec::new();
    let mut last_date = String::new();
    for (i, row) in json_rows.iter().enumerate() {
        if i % step != 0 && i != n - 1 { continue; }
        let xf = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
        let inner = row.trim_start_matches('[').trim_end_matches(']');
        if let Some(end_pos) = inner.find("\",") {
            let lbl = &inner[1..end_pos];
            let date = lbl.split(' ').next().unwrap_or(lbl);
            let time = lbl.split(' ').nth(1).unwrap_or("");
            if date != last_date {
                labels.push((xf, format!("{} {}", date, time)));
                last_date = date.to_string();
            } else {
                labels.push((xf, time.to_string()));
            }
        }
    }
    labels
}

fn chart_svg(
    title: &str, src: &str,
    datasets: &[(&str, &[(f64, f64)], &str, bool)],
    y_unit: &str, x_labels: &[(f64, String)],
    w: f64, h: f64,
) -> String {
    let ml = 55.0; let mr = 10.0; let mt = 10.0; let mb = 30.0;
    let pw = w - ml - mr;
    let ph = h - mt - mb;
    let refs: Vec<&[(f64, f64)]> = datasets.iter().map(|(_, d, _, _)| *d).collect();
    let (y_min, y_max) = combined_range(&refs);

    let mut svg = format!(
        "<div style=\"margin-bottom:1rem\"><h3 style=\"font-size:0.95rem;margin:0 0 2px\">{}</h3>\
         <span style=\"font-size:0.7rem;color:{}\">{}</span>\
         <svg viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\" style=\"width:100%;max-width:{}px;font-family:sans-serif\">",
        title, hc("6c757d"), src, w, h, w as u32
    );
    svg.push_str(&svg_axes(w, h, ml, mt, pw, ph, y_min, y_max, y_unit, x_labels, 5));
    for (_, data, color, fill) in datasets {
        svg.push_str(&svg_polyline(data, w, h, ml, mt, pw, ph, y_min, y_max, color, *fill));
    }
    // Legend
    let text_color = hc("212529");
    let mut lx = ml + 5.0;
    for (label, _, color, _) in datasets {
        svg.push_str(&format!(
            "<rect x=\"{:.0}\" y=\"2\" width=\"12\" height=\"3\" fill=\"{}\"/>\
             <text x=\"{:.0}\" y=\"8\" font-size=\"9\" fill=\"{}\">{}</text>",
            lx, color, lx + 15.0, text_color, label
        ));
        lx += 15.0 + label.len() as f64 * 5.5 + 10.0;
    }
    svg.push_str("</svg></div>");
    svg
}

#[allow(clippy::too_many_arguments)]
pub fn write_svg_report(
    f: &mut std::fs::File,
    start: &str, end: &str,
    json_rows: &[String],
    min_w: f64, min_w_time: &str, max_w: f64, max_w_time: &str,
    min_chill: f64, min_chill_time: &str, max_gust: f64, max_gust_time: &str,
    max_bft: u32, max_bft_time: &str, min_press: f64, min_press_time: &str,
    version: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let water_t = extract_col(json_rows, 1);
    let air_t = extract_col(json_rows, 2);
    let chill = extract_col(json_rows, 3);
    let dewpt = extract_col(json_rows, 4);
    let _humid = extract_col(json_rows, 5);
    let wind_s = extract_col(json_rows, 6);
    let gusts_d = extract_col(json_rows, 7);
    let wind_dir = extract_col(json_rows, 9);
    let pressure = extract_col(json_rows, 10);
    let water_lvl = extract_col(json_rows, 13);

    let x_labels = make_x_labels(json_rows, 16);
    let w = 900.0;
    let h = 250.0;
    let muted = hc("6c757d");

    // HTML head
    write!(f, "<!DOCTYPE html>\n<html lang=\"de\">\n<head>\n\
        <meta charset=\"UTF-8\">\n\
        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n\
        <title>Zürichsee — {} bis {}</title>\n", start, end)?;

    write!(f, "<style>\n\
        *{{margin:0;padding:0;box-sizing:border-box}}\n\
        body{{font-family:-apple-system,BlinkMacSystemFont,\"Segoe UI\",Roboto,sans-serif;background:#f8f9fa;color:#212529;line-height:1.6;padding:1rem;max-width:1000px;margin:0 auto}}\n\
        h1{{font-size:1.4rem;margin-bottom:.2rem}}\n\
        .sub{{color:#6c757d;font-size:.85rem;margin-bottom:1rem}}\n\
        .src{{color:#6c757d;font-size:.8rem;background:#fff;border:1px solid #dee2e6;border-radius:8px;padding:.5rem 1rem;margin-bottom:1rem}}\n\
        .src strong{{color:#212529}}\n\
        .sts{{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:.6rem;margin-bottom:1rem}}\n\
        .st{{background:#fff;border:1px solid #dee2e6;border-radius:8px;padding:.6rem .8rem}}\n\
        .st .l{{font-size:.65rem;color:#6c757d;text-transform:uppercase}}\n\
        .st .v{{font-size:1.3rem;font-weight:700}}\n\
        .st .u{{font-size:.75rem;color:#6c757d}}\n\
        table{{width:100%;border-collapse:collapse;font-size:.7rem;font-variant-numeric:tabular-nums}}\n\
        th,td{{padding:2px 5px;text-align:right;border-bottom:1px solid #dee2e6;white-space:nowrap}}\n\
        th{{background:#f8f9fa;position:sticky;top:0;font-weight:600}}\n\
        td:first-child,th:first-child{{text-align:left}}\n\
        .tw{{max-height:500px;overflow:auto;border:1px solid #dee2e6;border-radius:8px;margin-top:1rem}}\n\
        .dh{{background:#e9ecef;font-weight:700}}\n\
        footer{{margin-top:1.5rem;padding-top:.75rem;border-top:1px solid #dee2e6;font-size:.7rem;color:#6c757d}}\n\
        </style>\n</head>\n<body>\n")?;

    write!(f, "<h1>Zürichsee — Kombinierte Messwerte</h1>\n\
        <p class=\"sub\">{} bis {} · SVG-Report (kein JavaScript)</p>\n\
        <div class=\"src\">\n\
        <strong>Tiefenbrunnen (T):</strong> Wassertemp, Lufttemp, Windchill, Taupunkt, Feuchtigkeit, Wind, Böen, Beaufort, Windrichtung, Luftdruck<br>\n\
        <strong>Mythenquai (M):</strong> Niederschlag, Sonnenstrahlung, Pegel\n\
        </div>\n", start, end)?;

    // Stats
    write!(f, "<div class=\"sts\">\n\
        <div class=\"st\"><div class=\"l\">Wassertemp Min</div><div class=\"v\" style=\"color:#0d6efd\">{:.1} <span class=\"u\">°C</span></div><div class=\"l\">{}</div><div class=\"l\">Tiefenbrunnen</div></div>\n\
        <div class=\"st\"><div class=\"l\">Wassertemp Max</div><div class=\"v\" style=\"color:#dc3545\">{:.1} <span class=\"u\">°C</span></div><div class=\"l\">{}</div><div class=\"l\">Tiefenbrunnen</div></div>\n\
        <div class=\"st\"><div class=\"l\">Windchill Min</div><div class=\"v\" style=\"color:#0dcaf0\">{:.1} <span class=\"u\">°C</span></div><div class=\"l\">{}</div><div class=\"l\">Tiefenbrunnen</div></div>\n\
        <div class=\"st\"><div class=\"l\">Böen Max</div><div class=\"v\" style=\"color:#fd7e14\">{:.1} <span class=\"u\">m/s</span></div><div class=\"l\">{}</div><div class=\"l\">Tiefenbrunnen</div></div>\n\
        <div class=\"st\"><div class=\"l\">Beaufort Max</div><div class=\"v\" style=\"color:#fd7e14\">{} <span class=\"u\">bft</span></div><div class=\"l\">{}</div><div class=\"l\">Tiefenbrunnen</div></div>\n\
        <div class=\"st\"><div class=\"l\">Luftdruck Min</div><div class=\"v\" style=\"color:#6f42c1\">{:.0} <span class=\"u\">hPa</span></div><div class=\"l\">{}</div><div class=\"l\">Tiefenbrunnen</div></div>\n\
        </div>\n",
        min_w, min_w_time, max_w, max_w_time,
        min_chill, min_chill_time, max_gust, max_gust_time,
        max_bft, max_bft_time, min_press, min_press_time)?;

    // Charts
    let blue = &hc("0d6efd");
    let red = &hc("dc3545");
    let cyan = &hc("0dcaf0");
    let gray = &hc("6c757d");
    let green = &hc("198754");
    let orange = &hc("fd7e14");
    let purple = &hc("6f42c1");

    f.write_all(chart_svg("Temperaturverlauf", "Tiefenbrunnen (T)",
        &[("Wasser", &water_t, blue, true), ("Luft", &air_t, red, false),
          ("Windchill", &chill, cyan, false), ("Taupunkt", &dewpt, gray, false)],
        "°C", &x_labels, w, h).as_bytes())?;

    f.write_all(chart_svg("Wind &amp; Böen", "Tiefenbrunnen (T)",
        &[("Wind", &wind_s, green, false), ("Böen", &gusts_d, orange, true)],
        "m/s", &x_labels, w, h).as_bytes())?;

    // Wind direction scatter
    {
        let ml = 55.0; let mt = 10.0; let mb = 30.0; let mr = 10.0;
        let pw = w - ml - mr; let ph = h - mt - mb;
        let mut s = format!(
            "<div style=\"margin-bottom:1rem\"><h3 style=\"font-size:0.95rem;margin:0 0 2px\">Windrichtung</h3>\
             <span style=\"font-size:0.7rem;color:{}\">Tiefenbrunnen (T)</span>\
             <svg viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\" style=\"width:100%;max-width:{}px;font-family:sans-serif\">",
            muted, w, h, w as u32
        );
        s.push_str(&svg_axes(w, h, ml, mt, pw, ph, 0.0, 360.0, "°", &x_labels, 8));
        s.push_str(&svg_dots(&wind_dir, w, h, ml, mt, pw, ph, 0.0, 360.0, purple));
        for (deg, lbl) in [(0, "N"), (90, "O"), (180, "S"), (270, "W")] {
            let y = mt + ph - (deg as f64 / 360.0) * ph;
            s.push_str(&format!(
                "<text x=\"{:.0}\" y=\"{:.1}\" text-anchor=\"start\" font-size=\"9\" fill=\"{}\" opacity=\"0.6\">{}</text>",
                ml + 3.0, y + 3.0, muted, lbl
            ));
        }
        s.push_str("</svg></div>");
        f.write_all(s.as_bytes())?;
    }

    f.write_all(chart_svg("Luftdruck", "Tiefenbrunnen (T)",
        &[("Druck", &pressure, purple, false)],
        "hPa", &x_labels, w, h).as_bytes())?;

    f.write_all(chart_svg("Pegel", "Mythenquai (M)",
        &[("Pegel", &water_lvl, green, true)],
        "m ü.M.", &x_labels, w, h).as_bytes())?;

    // Data table
    write!(f, "<div class=\"tw\"><table><thead><tr>\
        <th>Zeit</th><th>Wasser °C (T)</th><th>Luft °C (T)</th><th>Chill °C (T)</th><th>Taupkt °C (T)</th>\
        <th>Feuchte % (T)</th><th>Wind m/s (T)</th><th>Böen m/s (T)</th><th>Bft (T)</th><th>Ri° (T)</th>\
        <th>Druck hPa (T)</th><th>Regen mm (M)</th><th>Strahl. (M)</th><th>Pegel m (M)</th>\
        </tr></thead><tbody>")?;

    let mut last_day = String::new();
    for row in json_rows {
        let inner = row.trim_start_matches('[').trim_end_matches(']');
        let mut fields: Vec<&str> = Vec::new();
        let mut in_q = false;
        let mut s = 0;
        for (i, c) in inner.char_indices() {
            if c == '"' { in_q = !in_q; }
            if c == ',' && !in_q { fields.push(&inner[s..i]); s = i + 1; }
        }
        fields.push(&inner[s..]);

        let label = fields.first().map(|s| s.trim().trim_matches('"')).unwrap_or("");
        let day = label.split(' ').next().unwrap_or("");
        let time = label.split(' ').nth(1).unwrap_or(label);

        if day != last_day {
            write!(f, "<tr class=\"dh\"><td colspan=\"14\">{}</td></tr>", day)?;
            last_day = day.to_string();
        }

        write!(f, "<tr><td>{}</td>", time)?;
        for i in 1..=13 {
            let val = fields.get(i).map(|s| s.trim().trim_matches('"')).unwrap_or("-");
            if val == "null" {
                write!(f, "<td>-</td>")?;
            } else if i == 9 {
                if let Ok(deg) = val.parse::<f64>() {
                    let dir = super::wind_direction_label(deg);
                    write!(f, "<td>{:.0}° {}</td>", deg, dir)?;
                } else {
                    write!(f, "<td>{}</td>", val)?;
                }
            } else {
                write!(f, "<td>{}</td>", val)?;
            }
        }
        write!(f, "</tr>")?;
    }

    write!(f, "</tbody></table></div>\n\
        <footer>Tiefenbrunnen (T) &amp; Mythenquai (M) — Wasserschutzpolizei Zürich · \
        SVG-Report generiert mit <strong>pegelstand</strong> CLI v{}</footer>\n\
        </body></html>", version)?;

    Ok(())
}

/// Standalone SVG: Pegelstand + Wassertemperatur + Lufttemperatur + Wind + Luftdruck (reine SVG-Datei, kein HTML)
pub fn write_standalone_svg(
    f: &mut std::fs::File,
    start: &str, end: &str,
    data: &[(String, f64, f64, f64, f64, f64, f64)], // (timestamp_label, water_temp, air_temp, water_level, wind_speed, wind_gust, pressure)
) -> Result<(), Box<dyn std::error::Error>> {
    let n = data.len();
    if n == 0 { return Err("Keine Daten".into()); }

    let w = 1000.0_f64;
    let ml = 90.0_f64;
    let mr = 20.0_f64;
    let mt = 40.0_f64;
    let pw = w - ml - mr;
    let ch_h = 180.0_f64;
    let gap = 45.0_f64;
    let ch1_top = mt;
    let ch2_top = mt + ch_h + gap;
    let ch3_top = ch2_top + ch_h + gap;
    let ch4_top = ch3_top + ch_h + gap;
    let total_h = ch4_top + ch_h + 50.0; // 50px for x-labels + footer

    // Build series
    let xf = |i: usize| -> f64 { if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 } };
    let water_temps: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.1.is_nan()).map(|(i, d)| (xf(i), d.1)).collect();
    let air_temps: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.2.is_nan()).map(|(i, d)| (xf(i), d.2)).collect();
    let water_levels: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.3.is_nan()).map(|(i, d)| (xf(i), d.3)).collect();
    let wind_speeds: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.4.is_nan()).map(|(i, d)| (xf(i), d.4)).collect();
    let wind_gusts: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.5.is_nan()).map(|(i, d)| (xf(i), d.5)).collect();
    let pressures: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.6.is_nan()).map(|(i, d)| (xf(i), d.6)).collect();

    // X-axis labels (date changes)
    let mut x_labels: Vec<(f64, String)> = Vec::new();
    let mut last_date = String::new();
    let step = std::cmp::max(1, n / 8);
    for (i, d) in data.iter().enumerate() {
        let lbl = &d.0;
        if i % step != 0 { continue; }
        let xf = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
        let date = lbl.split(' ').next().unwrap_or(lbl);
        let time = lbl.split(' ').nth(1).unwrap_or("");
        if date != last_date {
            x_labels.push((xf, format!("{}\\n{}", date, time)));
            last_date = date.to_string();
        } else {
            x_labels.push((xf, time.to_string()));
        }
    }

    let blue = &hc("0d6efd");
    let red = &hc("dc3545");
    let green = &hc("198754");
    let muted = &hc("6c757d");
    let gray = &hc("dee2e6");
    let text_color = &hc("212529");

    write!(f, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {total_h}" width="{w_int}" height="{h_int}" style="font-family:Helvetica,Arial,sans-serif;background:{bg}">
"#, w = w, total_h = total_h, w_int = w as u32, h_int = total_h as u32, bg = hc("ffffff"))?;

    // Title
    write!(f, "<text x=\"{}\" y=\"24\" text-anchor=\"middle\" font-size=\"16\" font-weight=\"bold\" fill=\"{}\">Zürichsee — {} bis {}</text>\n",
        w / 2.0, text_color, start, end)?;

    // Helper closure for drawing a chart panel
    fn draw_chart_panel(
        f: &mut std::fs::File, title: &str, y_unit: &str,
        datasets: &[(&str, &[(f64, f64)], &str, bool)],
        x_labels: &[(f64, String)],
        w: f64, ml: f64, _mr: f64, pw: f64, ch_h: f64, ch_top: f64,
        text_color: &str, muted: &str, gray: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let refs: Vec<&[(f64, f64)]> = datasets.iter().map(|(_, d, _, _)| *d).collect();
        let (y_min, y_max) = combined_range(&refs);

        write!(f, "<text x=\"{}\" y=\"{}\" font-size=\"12\" font-weight=\"600\" fill=\"{}\">{}  </text>\n",
            ml, ch_top - 2.0, text_color, title)?;

        for i in 0..=5u32 {
            let frac = i as f64 / 5.0;
            let y = ch_top + ch_h - frac * ch_h;
            let val = y_min + frac * (y_max - y_min);
            let suffix = if i == 5 { format!(" {}", y_unit) } else { String::new() };
            let label = if y_unit == "m ü.M." { format!("{:.2}", val) } else { format!("{:.1}", val) };
            write!(f, "<line x1=\"{}\" y1=\"{:.1}\" x2=\"{}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"0.5\"/>\n",
                ml, y, ml + pw, y, gray)?;
            write!(f, "<text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"11\" fill=\"{}\">{}{}</text>\n",
                ml - 6.0, y + 4.0, muted, label, suffix)?;
        }

        let label_count = x_labels.len();
        for (idx, (xf, label)) in x_labels.iter().enumerate() {
            let x = ml + xf * pw;
            write!(f, "<line x1=\"{:.1}\" y1=\"{}\" x2=\"{:.1}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"0.5\" stroke-dasharray=\"4,4\"/>\n",
                x, ch_top, x, ch_top + ch_h, gray)?;
            let anchor = if idx == 0 { "start" } else if idx == label_count - 1 { "end" } else { "middle" };
            if let Some((date_part, time_part)) = label.split_once("\\n") {
                write!(f, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"{}\" font-size=\"8\" fill=\"{}\">{}</text>\n",
                    x, ch_top + ch_h + 12.0, anchor, muted, date_part)?;
                write!(f, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"{}\" font-size=\"8\" fill=\"{}\">{}</text>\n",
                    x, ch_top + ch_h + 22.0, anchor, muted, time_part)?;
            } else {
                write!(f, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"{}\" font-size=\"9\" fill=\"{}\">{}</text>\n",
                    x, ch_top + ch_h + 15.0, anchor, muted, label)?;
            }
        }

        for (_, data, color, fill) in datasets {
            f.write_all(svg_polyline(data, w, ch_h, ml, ch_top, pw, ch_h, y_min, y_max, color, *fill).as_bytes())?;
        }

        // Legend
        let mut lx = ml + 5.0;
        let ly = ch_top + 14.0;
        for (name, _, color, _) in datasets {
            write!(f, "<rect x=\"{:.0}\" y=\"{:.0}\" width=\"14\" height=\"4\" fill=\"{}\"/>\n", lx, ly - 4.0, color)?;
            write!(f, "<text x=\"{:.0}\" y=\"{:.0}\" font-size=\"10\" fill=\"{}\">{}</text>\n", lx + 18.0, ly, text_color, name)?;
            lx += 18.0 + name.len() as f64 * 6.0 + 15.0;
        }
        Ok(())
    }

    // --- Chart 1: Temperatures ---
    draw_chart_panel(f, "Temperatur", "°C",
        &[("Wassertemperatur (T)", &water_temps, blue, true), ("Lufttemperatur (T)", &air_temps, red, false)],
        &x_labels, w, ml, mr, pw, ch_h, ch1_top, text_color, muted, gray)?;

    // --- Chart 2: Pegelstand ---
    if !water_levels.is_empty() {
        draw_chart_panel(f, "Pegelstand", "m ü.M.",
            &[("Pegel Mythenquai (M)", &water_levels, green, true)],
            &x_labels, w, ml, mr, pw, ch_h, ch2_top, text_color, muted, gray)?;
    }

    // --- Chart 3: Wind & Böen ---
    let orange = &hc("fd7e14");
    if !wind_speeds.is_empty() || !wind_gusts.is_empty() {
        draw_chart_panel(f, "Wind &amp; Böen", "m/s",
            &[("Wind (T)", &wind_speeds, green, false), ("Böen (T)", &wind_gusts, orange, true)],
            &x_labels, w, ml, mr, pw, ch_h, ch3_top, text_color, muted, gray)?;
    }

    // --- Chart 4: Luftdruck ---
    let purple = &hc("6f42c1");
    if !pressures.is_empty() {
        draw_chart_panel(f, "Luftdruck", "hPa",
            &[("Luftdruck (T)", &pressures, purple, false)],
            &x_labels, w, ml, mr, pw, ch_h, ch4_top, text_color, muted, gray)?;
    }

    // Footer
    write!(f, "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"9\" fill=\"{}\">Quellen: Tiefenbrunnen (T) + Mythenquai (M) — Wasserschutzpolizei Zürich (tecdottir.metaodi.ch) · pegelstand CLI</text>\n",
        w / 2.0, total_h - 6.0, muted)?;

    write!(f, "</svg>\n")?;
    Ok(())
}

/// Standalone SVG for Palea Fokea (Poseidon/HCMR) — 5 charts: Temp, Sea Level, Wind, Wind Dir, Pressure
pub fn write_paleafokea_svg(
    f: &mut std::fs::File,
    start: &str, end: &str,
    // (timestamp_label, air_temp, wind_speed, wind_dir, pressure, sea_level)
    data: &[(String, f64, f64, f64, f64, f64)],
) -> Result<(), Box<dyn std::error::Error>> {
    let n = data.len();
    if n == 0 { return Err("Keine Daten".into()); }

    let w = 1000.0_f64;
    let ml = 90.0_f64;
    let mr = 20.0_f64;
    let mt = 40.0_f64;
    let pw = w - ml - mr;
    let ch_h = 150.0_f64;
    let gap = 45.0_f64;
    let ch1_top = mt;
    let ch2_top = mt + ch_h + gap;
    let ch3_top = ch2_top + ch_h + gap;
    let ch4_top = ch3_top + ch_h + gap;
    let ch5_top = ch4_top + ch_h + gap;
    let total_h = ch5_top + ch_h + 50.0;

    let xf = |i: usize| -> f64 { if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 } };
    let air_temps: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.1.is_nan()).map(|(i, d)| (xf(i), d.1)).collect();
    let wind_speeds: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.2.is_nan()).map(|(i, d)| (xf(i), d.2)).collect();
    let wind_dirs: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.3.is_nan()).map(|(i, d)| (xf(i), d.3)).collect();
    let pressures: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.4.is_nan()).map(|(i, d)| (xf(i), d.4)).collect();
    let sea_levels: Vec<(f64, f64)> = data.iter().enumerate()
        .filter(|(_, d)| !d.5.is_nan()).map(|(i, d)| (xf(i), d.5)).collect();

    // X-axis labels
    let mut x_labels: Vec<(f64, String)> = Vec::new();
    let mut last_date = String::new();
    let step = std::cmp::max(1, n / 8);
    for (i, d) in data.iter().enumerate() {
        if i % step != 0 { continue; }
        let lbl = &d.0;
        let xf = if n > 1 { i as f64 / (n - 1) as f64 } else { 0.5 };
        let date = lbl.split(' ').next().unwrap_or(lbl);
        let time = lbl.split(' ').nth(1).unwrap_or("");
        if date != last_date {
            x_labels.push((xf, format!("{}\\n{}", date, time)));
            last_date = date.to_string();
        } else {
            x_labels.push((xf, time.to_string()));
        }
    }

    let blue = &hc("0d6efd");
    let red = &hc("dc3545");
    let green = &hc("198754");
    let orange = &hc("fd7e14");
    let purple = &hc("6f42c1");
    let muted = &hc("6c757d");
    let gray = &hc("dee2e6");
    let text_color = &hc("212529");

    write!(f, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {total_h}" width="{w_int}" height="{h_int}" style="font-family:Helvetica,Arial,sans-serif;background:{bg}">
"#, w = w, total_h = total_h, w_int = w as u32, h_int = total_h as u32, bg = hc("ffffff"))?;

    write!(f, "<text x=\"{}\" y=\"24\" text-anchor=\"middle\" font-size=\"16\" font-weight=\"bold\" fill=\"{}\">Palea Fokea — {} bis {}</text>\n",
        w / 2.0, text_color, start, end)?;

    fn draw_panel(
        f: &mut std::fs::File, title: &str, y_unit: &str,
        datasets: &[(&str, &[(f64, f64)], &str, bool)], // (name, data, color, fill)
        dots: Option<(&str, &[(f64, f64)], &str)>, // optional dot series (wind dir)
        fixed_range: Option<(f64, f64)>, // optional fixed y range (e.g. 0-360 for degrees)
        x_labels: &[(f64, String)],
        w: f64, ml: f64, _mr: f64, pw: f64, ch_h: f64, ch_top: f64,
        text_color: &str, muted: &str, gray: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (y_min, y_max) = if let Some((lo, hi)) = fixed_range {
            (lo, hi)
        } else {
            let mut all_data: Vec<&[(f64, f64)]> = datasets.iter().map(|(_, d, _, _)| *d).collect();
            if let Some((_, dd, _)) = &dots { all_data.push(*dd); }
            combined_range(&all_data)
        };

        write!(f, "<text x=\"{}\" y=\"{}\" font-size=\"12\" font-weight=\"600\" fill=\"{}\">{}  </text>\n",
            ml, ch_top - 2.0, text_color, title)?;

        let y_fmt = if y_unit == "m" && y_max < 5.0 { "2" } else if (y_max - y_min) >= 10.0 { "0" } else { "1" };
        for i in 0..=5u32 {
            let frac = i as f64 / 5.0;
            let y = ch_top + ch_h - frac * ch_h;
            let val = y_min + frac * (y_max - y_min);
            let suffix = if i == 5 { format!(" {}", y_unit) } else { String::new() };
            let label = match y_fmt { "2" => format!("{:.2}", val), "0" => format!("{:.0}", val), _ => format!("{:.1}", val) };
            write!(f, "<line x1=\"{}\" y1=\"{:.1}\" x2=\"{}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"0.5\"/>\n",
                ml, y, ml + pw, y, gray)?;
            write!(f, "<text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" font-size=\"11\" fill=\"{}\">{}{}</text>\n",
                ml - 6.0, y + 4.0, muted, label, suffix)?;
        }

        let label_count = x_labels.len();
        for (idx, (xf, label)) in x_labels.iter().enumerate() {
            let x = ml + xf * pw;
            write!(f, "<line x1=\"{:.1}\" y1=\"{}\" x2=\"{:.1}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"0.5\" stroke-dasharray=\"4,4\"/>\n",
                x, ch_top, x, ch_top + ch_h, gray)?;
            let anchor = if idx == 0 { "start" } else if idx == label_count - 1 { "end" } else { "middle" };
            if let Some((date_part, time_part)) = label.split_once("\\n") {
                write!(f, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"{}\" font-size=\"8\" fill=\"{}\">{}</text>\n",
                    x, ch_top + ch_h + 12.0, anchor, muted, date_part)?;
                write!(f, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"{}\" font-size=\"8\" fill=\"{}\">{}</text>\n",
                    x, ch_top + ch_h + 22.0, anchor, muted, time_part)?;
            } else {
                write!(f, "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"{}\" font-size=\"9\" fill=\"{}\">{}</text>\n",
                    x, ch_top + ch_h + 15.0, anchor, muted, label)?;
            }
        }

        for (_, data, color, fill) in datasets {
            f.write_all(svg_polyline(data, w, ch_h, ml, ch_top, pw, ch_h, y_min, y_max, color, *fill).as_bytes())?;
        }
        if let Some((_, dd, color)) = dots {
            f.write_all(svg_dots(dd, w, ch_h, ml, ch_top, pw, ch_h, y_min, y_max, color).as_bytes())?;
        }

        // Legend
        let mut lx = ml + 5.0;
        let ly = ch_top + 14.0;
        for (name, _, color, _) in datasets {
            write!(f, "<rect x=\"{:.0}\" y=\"{:.0}\" width=\"14\" height=\"4\" fill=\"{}\"/>\n", lx, ly - 4.0, color)?;
            write!(f, "<text x=\"{:.0}\" y=\"{:.0}\" font-size=\"10\" fill=\"{}\">{}</text>\n", lx + 18.0, ly, text_color, name)?;
            lx += 18.0 + name.len() as f64 * 6.0 + 15.0;
        }
        if let Some((name, _, color)) = dots {
            write!(f, "<circle cx=\"{:.0}\" cy=\"{:.0}\" r=\"3\" fill=\"{}\"/>\n", lx + 3.0, ly - 2.0, color)?;
            write!(f, "<text x=\"{:.0}\" y=\"{:.0}\" font-size=\"10\" fill=\"{}\">{}</text>\n", lx + 10.0, ly, text_color, name)?;
        }
        Ok(())
    }

    // Chart 1: Lufttemperatur
    draw_panel(f, "Lufttemperatur", "\u{00B0}C",
        &[("Lufttemperatur", &air_temps, red, true)], None, None,
        &x_labels, w, ml, mr, pw, ch_h, ch1_top, text_color, muted, gray)?;

    // Chart 2: Meeresspiegel
    draw_panel(f, "Meeresspiegel", "m",
        &[("Meeresspiegel", &sea_levels, blue, true)], None, None,
        &x_labels, w, ml, mr, pw, ch_h, ch2_top, text_color, muted, gray)?;

    // Chart 3: Windgeschwindigkeit
    draw_panel(f, "Windgeschwindigkeit", "m/s",
        &[("Wind", &wind_speeds, green, true)], None, None,
        &x_labels, w, ml, mr, pw, ch_h, ch3_top, text_color, muted, gray)?;

    // Chart 4: Windrichtung (dots, fixed 0-360°)
    draw_panel(f, "Windrichtung", "\u{00B0}",
        &[], Some(("Windrichtung", &wind_dirs, orange)), Some((0.0, 360.0)),
        &x_labels, w, ml, mr, pw, ch_h, ch4_top, text_color, muted, gray)?;

    // Chart 5: Luftdruck
    draw_panel(f, "Luftdruck", "hPa",
        &[("Luftdruck", &pressures, purple, false)], None, None,
        &x_labels, w, ml, mr, pw, ch_h, ch5_top, text_color, muted, gray)?;

    // Footer
    write!(f, "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"9\" fill=\"{}\">Quelle: POSEIDON/HCMR — Palea Fokea (37.72\u{00B0}N, 23.95\u{00B0}E) · pegelstand CLI</text>\n",
        w / 2.0, total_h - 6.0, muted)?;

    write!(f, "</svg>\n")?;
    Ok(())
}
