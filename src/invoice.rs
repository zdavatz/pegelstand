// Invoice PDF generator for Pump Züri pumpfoil lessons.
//
// Renders a one-page invoice matching the Google-Doc template
// (docs.google.com/document/d/1MZDJFtyfL0mOaawlR6VXjb5IgMUxNvoevFiol3LkaOE).
// Pure Rust via genpdf (DejaVu Sans embedded for umlauts) — same setup as
// src/bin/rechtsgrundlagen.rs. Only Datum (= lesson date) and the
// Rechnungsempfänger (name + mobile) vary per invoice.
//
// The Rechnungssteller identity (name/address, Ort, Twint payment number) is
// NOT hardcoded here — it holds a private phone number and this repo is public.
// It lives in the gitignored config `whatsapp/invoice-sender.txt` (override via
// $PEGELSTAND_INVOICE_SENDER), read at runtime by `load_sender()` — mirroring
// how `whatsapp/email-signature.txt` keeps the private number out of git.
//
// Font dir override via $FONT_DIR (default /usr/share/fonts/dejavu).

use std::path::{Path, PathBuf};

use genpdf::elements::{Break, Paragraph};
use genpdf::style::{Color, Style};
use genpdf::Alignment;

const DEFAULT_FONT_DIR: &str = "/usr/share/fonts/dejavu";
const MARGIN_MM: f64 = 22.0;

const INK: Color = Color::Rgb(20, 20, 20);
const GREY: Color = Color::Rgb(90, 90, 90);
const ACCENT: Color = Color::Rgb(0, 90, 140);

/// Fixed invoicing party, loaded from the gitignored config (never committed).
pub struct Sender {
    pub ort: String,               // z.B. "Zürich"
    pub zahlung: String,           // z.B. "Twint: 0XX XXX XX XX"
    pub absender_lines: Vec<String>, // Rechnungssteller-Block, mehrzeilig
}

/// The dynamic parts of one invoice.
pub struct Invoice<'a> {
    pub datum: &'a str,             // Lektionsdatum, z.B. "14.08.2026"
    pub betrag: &'a str,            // z.B. "CHF 65.-"
    pub empfaenger_name: &'a str,   // "Vorname Nachname"
    pub empfaenger_mobile: &'a str, // Mobilnummer
}

/// Locate the invoice-sender config: $PEGELSTAND_INVOICE_SENDER first, else
/// whatsapp/invoice-sender.txt next to the crate.
fn sender_path() -> PathBuf {
    if let Ok(p) = std::env::var("PEGELSTAND_INVOICE_SENDER") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return pb;
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("whatsapp/invoice-sender.txt")
}

/// Parse the sender config. Format:
///   Ort: <city>
///   Zahlung: <payment string>
///   Absender:
///   <line 1>
///   <line 2>
///   ...
/// Lines before `Absender:` are `Key: value`; everything after is the
/// multi-line Rechnungssteller block.
pub fn load_sender() -> Result<Sender, Box<dyn std::error::Error>> {
    let path = sender_path();
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "Rechnungs-Absender fehlt ({}): {}. Bitte die gitignorte Datei anlegen \
             (Zeilen 'Ort:', 'Zahlung:', dann 'Absender:' + Adressblock).",
            path.display(),
            e
        )
    })?;

    let mut ort = String::new();
    let mut zahlung = String::new();
    let mut absender_lines: Vec<String> = Vec::new();
    let mut in_absender = false;
    for line in raw.lines() {
        if in_absender {
            let t = line.trim_end();
            if !t.trim().is_empty() {
                absender_lines.push(t.to_string());
            }
            continue;
        }
        let t = line.trim();
        if t.eq_ignore_ascii_case("Absender:") {
            in_absender = true;
        } else if let Some(v) = t.strip_prefix("Ort:") {
            ort = v.trim().to_string();
        } else if let Some(v) = t.strip_prefix("Zahlung:") {
            zahlung = v.trim().to_string();
        }
    }

    if ort.is_empty() || zahlung.is_empty() || absender_lines.is_empty() {
        return Err(format!(
            "Rechnungs-Absender unvollständig in {} (Ort/Zahlung/Absender-Block erforderlich).",
            path.display()
        )
        .into());
    }
    Ok(Sender { ort, zahlung, absender_lines })
}

fn font_dir() -> String {
    std::env::var("FONT_DIR").unwrap_or_else(|_| DEFAULT_FONT_DIR.to_string())
}

fn load_font_family(
    font_dir: &str,
) -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>, Box<dyn std::error::Error>> {
    let load = |file: &str| -> Result<genpdf::fonts::FontData, Box<dyn std::error::Error>> {
        let path = Path::new(font_dir).join(file);
        let data = std::fs::read(&path)
            .map_err(|e| format!("Font {} nicht lesbar: {}", path.display(), e))?;
        genpdf::fonts::FontData::new(data, None)
            .map_err(|e| format!("Font {} nicht parsebar: {}", file, e).into())
    };
    Ok(genpdf::fonts::FontFamily {
        regular: load("DejaVuSans.ttf")?,
        bold: load("DejaVuSans-Bold.ttf")?,
        italic: load("DejaVuSans-Oblique.ttf")?,
        bold_italic: load("DejaVuSans-BoldOblique.ttf")?,
    })
}

fn line(doc: &mut genpdf::Document, text: &str, style: Style, align: Alignment) {
    let mut p = Paragraph::default();
    p.push_styled(text.to_string(), style);
    doc.push(p.aligned(align));
}

/// Render the invoice to `out_path` (a .pdf). Overwrites if it exists.
pub fn render_invoice_pdf(
    inv: &Invoice,
    sender: &Sender,
    out_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let family = load_font_family(&font_dir())?;
    let mut doc = genpdf::Document::new(family);
    doc.set_title("Rechnung Pumpfoilen");
    doc.set_minimal_conformance();
    let mut deco = genpdf::SimplePageDecorator::new();
    deco.set_margins(MARGIN_MM);
    doc.set_page_decorator(deco);

    let body = Style::new().with_color(INK).with_font_size(11);
    let head = Style::new().with_color(ACCENT).with_font_size(11).bold();

    // Titel
    line(
        &mut doc,
        "Rechnung Pumpfoilen",
        Style::new().with_color(INK).with_font_size(20).bold(),
        Alignment::Left,
    );
    doc.push(Break::new(1.5));

    // Datum / Ort
    line(&mut doc, &format!("Datum: {}", inv.datum), body, Alignment::Left);
    line(&mut doc, &format!("Ort: {}", sender.ort), body, Alignment::Left);
    doc.push(Break::new(1.0));

    // Rechnungssteller (aus Config)
    line(&mut doc, "Rechnungssteller:", head, Alignment::Left);
    for l in &sender.absender_lines {
        line(&mut doc, l, body, Alignment::Left);
    }
    doc.push(Break::new(1.0));

    // Betrag / Zahlung
    line(
        &mut doc,
        &format!("Betrag: {}", inv.betrag),
        Style::new().with_color(INK).with_font_size(11).bold(),
        Alignment::Left,
    );
    line(&mut doc, &format!("Zahlung: {}", sender.zahlung), body, Alignment::Left);
    doc.push(Break::new(1.0));

    // Rechnungsempfänger
    line(&mut doc, "Rechnungsempfänger:", head, Alignment::Left);
    line(&mut doc, inv.empfaenger_name, body, Alignment::Left);
    line(&mut doc, inv.empfaenger_mobile, body, Alignment::Left);
    doc.push(Break::new(2.0));

    // Fusszeile
    line(
        &mut doc,
        "Vielen Dank und bis bald am Wasser!",
        Style::new().with_color(GREY).with_font_size(10).italic(),
        Alignment::Left,
    );

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    doc.render_to_file(out_path)
        .map_err(|e| format!("PDF-Render {}: {}", out_path.display(), e))?;
    Ok(())
}
