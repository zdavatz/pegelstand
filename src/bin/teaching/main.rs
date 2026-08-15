// How I Teach Pumpfoiling — a compact teaching guide by Zeno Davatz (Pump
// Tsüri), compiled from his own published statements and direct input: the
// belly-knee-feet progression (pump.zuerich blog + author's numbers), the
// "Up On Foil" Ep. 11 podcast interview, the "Pump Tsüri by Boardtales"
// video, the pump.zuerich posts on the therapeutic value of pumpfoiling, and
// the Instagram posts — around the core rule: "no ambitions, no expectations".
// Deliberately short on text and heavy on video links (author's feedback:
// "zu viel Text, es braucht mehr Videos").
//
// One PDF per language: EN (original) plus DE, FR, IT, ES, ZH, JA, EL —
// all texts live in the texts_*.rs modules, the document structure and the
// link order are identical across languages.
//
// Pure Rust, no Chrome: the PDF is produced directly with `genpdf` (DejaVu
// Sans embedded; Arial Unicode for ZH/JA), then reopened with `lopdf` to
// overlay clickable /Link URI annotations on every URL line — genpdf 0.2
// cannot emit hyperlinks itself. Same infrastructure as `rechtsgrundlagen.rs`.
//
//   cargo run --release --bin teaching             # all languages
//   cargo run --release --bin teaching -- ZH JA    # a subset
//
// Override the DejaVu font directory with $FONT_DIR (default
// /usr/share/fonts/dejavu). ZH/JA need a CJK TrueType font: $CJK_FONT
// (default is the full 23 MB Arial Unicode — generate a small subset with
// teaching/make_cjk_subset.py and point $CJK_FONT at it for small PDFs).

mod i18n;
mod texts_de;
mod texts_el;
mod texts_en;
mod texts_es;
mod texts_fr;
mod texts_it;
mod texts_ja;
mod texts_ko;
mod texts_zh;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use genpdf::elements::{Break, Paragraph};
use genpdf::style::{Color, Style};
use genpdf::Alignment;
use i18n::T;

const DEFAULT_FONT_DIR: &str = "/usr/share/fonts/dejavu";
const DEFAULT_CJK_FONT: &str = "/System/Library/Fonts/Supplemental/Arial Unicode.ttf";
const OUT_DIR: &str = "teaching";

// A4 geometry (points) and margins, used to span the clickable rectangles
// and to compute the CJK wrap width.
const A4_WIDTH_PT: f64 = 595.276;
const MARGIN_MM: u8 = 20;
const CONTENT_WIDTH_PT: f64 = A4_WIDTH_PT - 2.0 * (MARGIN_MM as f64) * 72.0 / 25.4;

// URL lines are the ONLY lines set in this font size. That is how add_links()
// finds them again in the content stream (the text itself is CID-encoded, but
// the position operators are plain numbers). No other text size may be 9.
const LINK_FONT_SIZE: u8 = 9;

// Palette (same family as the Rechtsgrundlagen dossier).
const INK: Color = Color::Rgb(0x1a, 0x1a, 0x1a);
const ACCENT: Color = Color::Rgb(0x0d, 0x47, 0x6b); // deep lake blue
const GOLD: Color = Color::Rgb(0x9a, 0x7b, 0x2e);
const GREY: Color = Color::Rgb(0x55, 0x55, 0x55);
const LINKCOL: Color = Color::Rgb(0x12, 0x5a, 0x9c);
const QUOTECOL: Color = Color::Rgb(0x33, 0x33, 0x33);

// ---- Sources (all made clickable, identical in every language) ------------
const URL_PUMP: &str = "https://pump.zuerich";
const URL_IG_ZDAVATZ: &str = "https://www.instagram.com/zdavatz/";
const URL_IG_KNEESTART: &str = "https://www.instagram.com/p/DY_u6dlMwi9/";
const URL_POOL: &str = "https://pump.zuerich/2025/01/15/pool-pump-zurich-every-friday-for-lunch/";
const URL_ONIX_PACKS: &str = "https://www.onix-foils.com/products/pumpfoil-packs";
const URL_TOTALSUP: &str = "https://www.totalsup.com/news/can-pump-foiling-become-switzerland-national-sport-indiana-paddle-surf-zeno-davatz/";
const URL_BLOG_BKF: &str = "https://pump.zuerich/2025/03/24/belly-knee-feet/";
const URL_SCHULUNG: &str = "https://pump.zuerich/schulung/";
const URL_UPONFOIL: &str = "https://www.youtube.com/watch?v=aRPzhw6CClk";
const URL_BOARDTALES: &str = "https://www.youtube.com/watch?v=L5epbkKa8pU";
const URL_IG_ADHD: &str = "https://www.instagram.com/p/DIzCftbPPra/";
const URL_BLOG_ADHD: &str = "https://pump.zuerich/2025/04/24/pumpfoil-your-adhd/";
const URL_BLOG_THERAPY: &str =
    "https://pump.zuerich/2023/06/27/the-therapeutic-value-of-pumpfoiling-dockstarting/";

// Shortened display texts for URLs too long for one line (see linkline()).
const DISP_TOTALSUP: &str = "totalsup.com → Can pump foiling become Switzerland's national sport?";
const DISP_POOL: &str = "pump.zuerich → Pool Pump Zürich every Friday for Lunch";
const DISP_THERAPY: &str = "pump.zuerich → The therapeutic value of Pumpfoiling/Dockstarting";

/// Collects the document and remembers the order of the URL lines so that
/// add_links() can match them to the text positions found afterwards.
struct Builder {
    doc: genpdf::Document,
    links: Vec<String>,
    cjk: bool,
    qopen: &'static str,
    qclose: &'static str,
}

impl Builder {
    fn break_(&mut self, n: f64) {
        self.doc.push(Break::new(n));
    }

    fn line(&mut self, text: &str, style: Style, align: Alignment) {
        let mut p = Paragraph::default();
        p.push_styled(text.to_string(), style);
        self.doc.push(p.aligned(align));
    }

    /// Pre-wraps a line for CJK languages (genpdf only breaks at spaces and
    /// silently DROPS an unbreakable over-wide word). No-op otherwise.
    fn wrap(&self, text: &str, font_size: f64) -> Vec<String> {
        if !self.cjk {
            return vec![text.to_string()];
        }
        cjk_wrap(text, CONTENT_WIDTH_PT / font_size - 2.0)
    }

    /// Multi-line body text (every `\n` line becomes its own paragraph).
    fn body(&mut self, text: &str) {
        for l in text.split('\n') {
            for wl in self.wrap(l, 10.0) {
                self.line(&wl, Style::new().with_color(INK).with_font_size(10), Alignment::Left);
            }
        }
    }

    fn h1(&mut self, text: &str) {
        self.break_(0.8);
        for wl in self.wrap(text, 15.0) {
            self.line(&wl, Style::new().with_color(ACCENT).with_font_size(15).bold(), Alignment::Left);
        }
        self.break_(0.4);
    }

    /// Verbatim quote (indented, italic) with attribution.
    fn quote(&mut self, text: &str, who: &str) {
        self.break_(0.2);
        let style = Style::new().with_color(QUOTECOL).with_font_size(10).italic();
        if self.cjk {
            let full = format!("    {}{}{}", self.qopen, text.trim(), self.qclose);
            for wl in self.wrap(&full, 10.0) {
                self.line(&wl, style, Alignment::Left);
            }
        } else {
            for l in text.split('\n') {
                self.line(&format!("    {}{}{}", self.qopen, l.trim(), self.qclose), style, Alignment::Left);
            }
        }
        self.line(
            &format!("    — {}", who),
            Style::new().with_color(GREY).with_font_size(8).italic(),
            Alignment::Left,
        );
        self.break_(0.3);
    }

    /// A link line: `display` is set visibly in the reserved link font size
    /// (marker for add_links), `url` is stored as the target. genpdf silently
    /// drops a line whose single unbreakable word is wider than the column —
    /// long URLs therefore need a shorter `display` text.
    fn linkline(&mut self, display: &str, url: &str, align: Alignment) {
        self.line(display, Style::new().with_color(LINKCOL).with_font_size(LINK_FONT_SIZE), align);
        self.links.push(url.to_string());
    }

    /// Source line: label as body text, the link underneath.
    fn source_disp(&mut self, label: &str, display: &str, url: &str) {
        self.body(label);
        self.linkline(display, url, Alignment::Left);
        self.break_(0.25);
    }
}

/// True for glyphs set on a full em in CJK fonts (ideographs, kana, Hangul,
/// fullwidth punctuation — plus the em dash and curly quotes, which Arial
/// Unicode also draws em-wide).
fn cjk_is_wide(c: char) -> bool {
    let u = c as u32;
    (0x2E80..=0xA4CF).contains(&u)
        || (0x1100..=0x11FF).contains(&u) // Hangul Jamo
        || (0xAC00..=0xD7A3).contains(&u) // Hangul syllables
        || (0xF900..=0xFAFF).contains(&u)
        || (0xFE30..=0xFE4F).contains(&u)
        || (0xFF00..=0xFF60).contains(&u)
        || (0x20000..=0x2FA1F).contains(&u)
        || matches!(u, 0x2013 | 0x2014 | 0x2018 | 0x2019 | 0x201C | 0x201D | 0x2026)
}

/// Estimated width in ems (CJK glyph = 1 em, ASCII ≈ 0.53 em, space 0.3 em).
fn est_ems(s: &str) -> f64 {
    s.chars()
        .map(|c| {
            if cjk_is_wide(c) {
                1.0
            } else if c == ' ' {
                0.3
            } else {
                0.53
            }
        })
        .sum()
}

/// Greedy line wrap for Chinese/Japanese text (which has no spaces to break
/// at): every CJK glyph is a break opportunity, ASCII runs stay atomic.
/// Simple kinsoku: a line never starts with closing punctuation (it hangs
/// over the edge instead — max_ems leaves 2 ems of slack for that) and never
/// ends with an opening bracket/quote (it is carried to the next line).
fn cjk_wrap(text: &str, max_ems: f64) -> Vec<String> {
    const NO_START: &str = "、。，．！？：；）」』”’…・ー々〜—％);:,.!?]";
    const NO_END: &str = "（「『“‘([";
    if text.trim().is_empty() {
        return vec![text.to_string()];
    }

    let mut toks: Vec<String> = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c == ' ' || cjk_is_wide(c) {
            if !word.is_empty() {
                toks.push(std::mem::take(&mut word));
            }
            toks.push(c.to_string());
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        toks.push(word);
    }

    let mut lines: Vec<String> = vec![String::new()];
    let mut w = 0.0f64;
    for tok in toks {
        let tw = est_ems(&tok);
        let cur = lines.last_mut().unwrap();
        if w + tw <= max_ems || cur.is_empty() {
            cur.push_str(&tok);
            w += tw;
            continue;
        }
        if tok == " " {
            lines.push(String::new());
            w = 0.0;
            continue;
        }
        if tok.chars().count() == 1 && NO_START.contains(tok.chars().next().unwrap()) {
            cur.push_str(&tok); // hang over the right edge
            w += tw;
            continue;
        }
        let mut nl = String::new();
        if let Some(lc) = cur.chars().last() {
            if NO_END.contains(lc) {
                cur.pop();
                nl.push(lc);
            }
        }
        nl.push_str(&tok);
        w = est_ems(&nl);
        lines.push(nl);
    }
    for l in lines.iter_mut().skip(1) {
        while l.starts_with(' ') {
            l.remove(0);
        }
    }
    lines
}

fn load_dejavu(font_dir: &str) -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>> {
    let load = |file: &str| -> Result<genpdf::fonts::FontData> {
        let path = Path::new(font_dir).join(file);
        let data = std::fs::read(&path).map_err(|e| anyhow!("read font {}: {}", path.display(), e))?;
        genpdf::fonts::FontData::new(data, None).map_err(|e| anyhow!("parse font {}: {}", file, e))
    };
    Ok(genpdf::fonts::FontFamily {
        regular: load("DejaVuSans.ttf")?,
        bold: load("DejaVuSans-Bold.ttf")?,
        italic: load("DejaVuSans-Oblique.ttf")?,
        bold_italic: load("DejaVuSans-BoldOblique.ttf")?,
    })
}

/// ZH/JA: one CJK TrueType font for all four styles (Arial Unicode has no
/// bold/italic siblings; headings still stand out via size and colour).
fn load_cjk() -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>> {
    let path = std::env::var("CJK_FONT").unwrap_or_else(|_| DEFAULT_CJK_FONT.into());
    let data = std::fs::read(&path).map_err(|e| anyhow!("read CJK font {}: {}", path, e))?;
    let mk = || {
        genpdf::fonts::FontData::new(data.clone(), None)
            .map_err(|e| anyhow!("parse CJK font {}: {}", path, e))
    };
    Ok(genpdf::fonts::FontFamily { regular: mk()?, bold: mk()?, italic: mk()?, bold_italic: mk()? })
}

fn build(t: &T, font_dir: &str) -> Result<()> {
    let family = if t.cjk { load_cjk()? } else { load_dejavu(font_dir)? };
    let mut doc = genpdf::Document::new(family);
    doc.set_title(t.meta_title);
    doc.set_minimal_conformance();
    let mut deco = genpdf::SimplePageDecorator::new();
    deco.set_margins(MARGIN_MM);
    doc.set_page_decorator(deco);

    let mut b = Builder { doc, links: Vec::new(), cjk: t.cjk, qopen: t.qopen, qclose: t.qclose };

    // ===================== Title (no separate title page) =================
    b.break_(0.5);
    b.line("PUMP TSÜRI", Style::new().with_color(GOLD).with_font_size(11).bold(), Alignment::Center);
    b.break_(0.6);
    b.line(t.title, Style::new().with_color(INK).with_font_size(22).bold(), Alignment::Center);
    b.break_(0.5);
    b.line(t.tagline, Style::new().with_color(ACCENT).with_font_size(13).italic(), Alignment::Center);
    b.break_(0.6);
    b.line(t.byline, Style::new().with_color(GREY).with_font_size(10), Alignment::Center);
    b.break_(0.3);
    b.linkline(URL_PUMP, URL_PUMP, Alignment::Center);
    b.linkline(URL_IG_ZDAVATZ, URL_IG_ZDAVATZ, Alignment::Center);
    b.break_(0.2);
    b.line(
        &t.version_line.replace("{}", &today()),
        Style::new().with_color(GREY).with_font_size(8),
        Alignment::Center,
    );

    // ===================== 1. The three things ============================
    b.h1(t.s1_h);
    b.quote(t.s1_q, t.s1_att);
    b.body(t.s1_b);

    // ===================== 2. Why this sport ==============================
    b.h1(t.s2_h);
    b.body(t.s2_b1);
    b.quote(t.s2_q1, t.s2_a1);
    b.body(t.s2_b2);
    b.quote(t.s2_q2, t.s2_a2);
    b.body(t.s2_b3);

    // ===================== 3. The progression =============================
    b.h1(t.s3_h);
    b.body(t.s3_b);

    // ===================== 4. Rules at the dock ===========================
    b.h1(t.s4_h);
    b.body(t.s4_b);

    // ===================== 5. Safety ======================================
    b.h1(t.s5_h);
    b.body(t.s5_b);

    // ===================== 6. The gear ====================================
    b.h1(t.s6_h);
    b.body(t.s6_b);
    b.break_(0.2);
    b.source_disp(t.s6_onix, URL_ONIX_PACKS, URL_ONIX_PACKS);

    // ===================== 7. Watch, then jump ============================
    b.h1(t.s7_h);
    b.body(t.s7_intro);
    b.break_(0.2);
    b.source_disp(t.sl_bkf, URL_BLOG_BKF, URL_BLOG_BKF);
    b.source_disp(t.sl_kneestart, URL_IG_KNEESTART, URL_IG_KNEESTART);
    b.source_disp(t.sl_boardtales, URL_BOARDTALES, URL_BOARDTALES);
    b.source_disp(t.sl_uponfoil, URL_UPONFOIL, URL_UPONFOIL);
    b.source_disp(t.sl_totalsup, DISP_TOTALSUP, URL_TOTALSUP);
    b.source_disp(t.sl_schulung, URL_SCHULUNG, URL_SCHULUNG);
    b.source_disp(t.sl_pool, DISP_POOL, URL_POOL);
    b.source_disp(t.sl_adhd, URL_BLOG_ADHD, URL_BLOG_ADHD);
    b.source_disp(t.sl_therapy, DISP_THERAPY, URL_BLOG_THERAPY);
    b.source_disp(t.sl_ig, URL_IG_ADHD, URL_IG_ADHD);

    b.break_(0.6);
    b.line(t.closing, Style::new().with_color(ACCENT).with_font_size(10).italic(), Alignment::Left);

    // ---- Render + links --------------------------------------------------
    std::fs::create_dir_all(OUT_DIR)?;
    let out = PathBuf::from(OUT_DIR).join(t.out_file);
    let links = b.links.clone();
    b.doc.render_to_file(&out).map_err(|e| anyhow!("render {}: {}", out.display(), e))?;
    let n = add_links(&out, &links)?;
    eprintln!("wrote {} ({} clickable links)", out.display(), n);
    Ok(())
}

fn today() -> String {
    use chrono::Local;
    Local::now().format("%d.%m.%Y").to_string()
}

/// Overlays clickable /Link URI annotations on all URL lines. genpdf 0.2
/// cannot emit hyperlinks; the finished PDF is therefore reopened with lopdf,
/// every text line set in LINK_FONT_SIZE is located across all pages (in
/// reading order) and matched, in order, to the collected URLs. The rectangle
/// spans the full line width so it works for left- and centre-aligned lines.
fn add_links(pdf_path: &Path, urls: &[String]) -> Result<usize> {
    use lopdf::{Dictionary, Document, Object, StringFormat};

    let mut doc = Document::load(pdf_path)?;

    let num = |o: &Object| -> Option<f64> {
        match o {
            Object::Real(r) => Some(*r as f64),
            Object::Integer(i) => Some(*i as f64),
            _ => None,
        }
    };

    let mut hits: Vec<(lopdf::ObjectId, f64)> = Vec::new();
    let pages = doc.get_pages(); // BTreeMap<u32, ObjectId> — sorted by page number
    for (_pno, page_id) in pages {
        let content = doc.get_and_decode_page_content(page_id)?;
        let mut pos = (0.0f64, 0.0f64);
        let mut size = 0.0f64;
        let mut last: Option<f64> = None;
        for op in &content.operations {
            match op.operator.as_str() {
                "Td" | "TD" if op.operands.len() >= 2 => {
                    if let (Some(x), Some(y)) = (num(&op.operands[0]), num(&op.operands[1])) {
                        pos = (x, y);
                    }
                }
                "Tm" if op.operands.len() >= 6 => {
                    if let (Some(x), Some(y)) = (num(&op.operands[4]), num(&op.operands[5])) {
                        pos = (x, y);
                    }
                }
                "Tf" if op.operands.len() >= 2 => {
                    if let Some(s) = num(&op.operands[1]) {
                        size = s;
                    }
                }
                "Tj" | "TJ" => {
                    if (size - LINK_FONT_SIZE as f64).abs() < 0.01 && last != Some(pos.1) {
                        hits.push((page_id, pos.1));
                        last = Some(pos.1);
                    }
                }
                _ => {}
            }
        }
    }

    let mut added = 0usize;
    for ((page_id, y), url) in hits.iter().zip(urls.iter()) {
        let mut action = Dictionary::new();
        action.set("S", Object::Name(b"URI".to_vec()));
        action.set("URI", Object::String(url.as_bytes().to_vec(), StringFormat::Literal));
        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"Link".to_vec()));
        annot.set(
            "Rect",
            Object::Array(vec![
                Object::Real(38.0),
                Object::Real((*y - 2.0) as f32),
                Object::Real((A4_WIDTH_PT - 38.0) as f32),
                Object::Real((*y + LINK_FONT_SIZE as f64 + 2.0) as f32),
            ]),
        );
        annot.set("Border", Object::Array(vec![0.into(), 0.into(), 0.into()]));
        annot.set("A", Object::Dictionary(action));
        let id = doc.add_object(annot);
        let page = doc.get_object_mut(*page_id)?.as_dict_mut()?;
        match page.get_mut(b"Annots") {
            Ok(Object::Array(arr)) => arr.push(Object::Reference(id)),
            _ => page.set("Annots", Object::Array(vec![Object::Reference(id)])),
        }
        added += 1;
    }

    if hits.len() != urls.len() {
        eprintln!(
            "  warning: {} link lines found but {} URLs collected — mapping may be incomplete.",
            hits.len(),
            urls.len()
        );
    }

    doc.save(pdf_path)?;
    Ok(added)
}

fn main() -> Result<()> {
    let font_dir = std::env::var("FONT_DIR").unwrap_or_else(|_| DEFAULT_FONT_DIR.into());
    let only: Vec<String> = std::env::args().skip(1).map(|a| a.to_uppercase()).collect();
    for t in i18n::all() {
        if !only.is_empty() && !only.iter().any(|c| c == t.code) {
            continue;
        }
        build(&t, &font_dir)?;
    }
    Ok(())
}
