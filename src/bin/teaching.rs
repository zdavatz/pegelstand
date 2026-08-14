// How I Teach Pumpfoiling — a compact teaching guide by Zeno Davatz (Pump
// Tsüri), compiled from his own published statements and direct input: the
// belly-knee-feet progression (pump.zuerich blog + author's numbers), the
// "Up On Foil" Ep. 11 podcast interview, the "Pump Tsüri by Boardtales"
// video, the pump.zuerich posts on the therapeutic value of pumpfoiling, and
// the Instagram posts — around the core rule: "no ambitions, no expectations".
// Deliberately short on text and heavy on video links (author's feedback:
// "zu viel Text, es braucht mehr Videos").
//
// Pure Rust, no Chrome: the PDF is produced directly with `genpdf` (DejaVu
// Sans embedded), then reopened with `lopdf` to overlay clickable /Link URI
// annotations on every URL line — genpdf 0.2 cannot emit hyperlinks itself.
// Same infrastructure as `rechtsgrundlagen.rs`.
//
//   cargo run --release --bin teaching
//
// Override the font directory with $FONT_DIR (default /usr/share/fonts/dejavu).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use genpdf::elements::{Break, Paragraph};
use genpdf::style::{Color, Style};
use genpdf::Alignment;

const DEFAULT_FONT_DIR: &str = "/usr/share/fonts/dejavu";
const OUT_DIR: &str = "teaching";
const OUT_FILE: &str = "How_I_Teach_Pumpfoiling.pdf";

// A4 geometry (points) and margins, used to span the clickable rectangles.
const A4_WIDTH_PT: f64 = 595.276;
const MARGIN_MM: u8 = 20;

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

// ---- Sources (all made clickable) -----------------------------------------
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

/// Collects the document and remembers the order of the URL lines so that
/// add_links() can match them to the text positions found afterwards.
struct Builder {
    doc: genpdf::Document,
    links: Vec<String>,
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

    /// Multi-line body text (every `\n` line becomes its own paragraph).
    fn body(&mut self, text: &str) {
        for l in text.split('\n') {
            self.line(l, Style::new().with_color(INK).with_font_size(10), Alignment::Left);
        }
    }

    fn h1(&mut self, text: &str) {
        self.break_(0.8);
        self.line(text, Style::new().with_color(ACCENT).with_font_size(15).bold(), Alignment::Left);
        self.break_(0.4);
    }

    /// Verbatim quote (indented, italic) with attribution.
    fn quote(&mut self, text: &str, who: &str) {
        self.break_(0.2);
        for l in text.split('\n') {
            self.line(
                &format!("    “{}”", l.trim()),
                Style::new().with_color(QUOTECOL).with_font_size(10).italic(),
                Alignment::Left,
            );
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
        self.line(label, Style::new().with_color(INK).with_font_size(10), Alignment::Left);
        self.linkline(display, url, Alignment::Left);
        self.break_(0.25);
    }
}

fn load_font_family(font_dir: &str) -> Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>> {
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

fn build(font_dir: &str) -> Result<()> {
    let family = load_font_family(font_dir)?;
    let mut doc = genpdf::Document::new(family);
    doc.set_title("How I Teach Pumpfoiling");
    doc.set_minimal_conformance();
    let mut deco = genpdf::SimplePageDecorator::new();
    deco.set_margins(MARGIN_MM);
    doc.set_page_decorator(deco);

    let mut b = Builder { doc, links: Vec::new() };

    // ===================== Title (no separate title page) =================
    b.break_(0.5);
    b.line("PUMP TSÜRI", Style::new().with_color(GOLD).with_font_size(11).bold(), Alignment::Center);
    b.break_(0.6);
    b.line(
        "How I Teach Pumpfoiling",
        Style::new().with_color(INK).with_font_size(22).bold(),
        Alignment::Center,
    );
    b.break_(0.5);
    b.line(
        "No ambitions, no expectations.",
        Style::new().with_color(ACCENT).with_font_size(13).italic(),
        Alignment::Center,
    );
    b.break_(0.6);
    b.line(
        "Zeno Davatz — teaching pumpfoiling to kids and adults in Zürich since 2020",
        Style::new().with_color(GREY).with_font_size(10),
        Alignment::Center,
    );
    b.break_(0.3);
    b.linkline(URL_PUMP, URL_PUMP, Alignment::Center);
    b.linkline(URL_IG_ZDAVATZ, URL_IG_ZDAVATZ, Alignment::Center);
    b.break_(0.2);
    b.line(
        &format!("Version: {} · This is a guide to read once — then go watch the videos and pump.", today()),
        Style::new().with_color(GREY).with_font_size(8),
        Alignment::Center,
    );

    // ===================== 1. The three things ============================
    b.h1("1. The Three Things I Always Say");
    b.quote(
        "First: no expectations. Second: no ambition. And third — this one is for the adults — \
it's like matrimonial therapy. You can't tell your wife or your husband what to do. You can \
only work on yourself. So you can't tell the board what the board should do.",
        "Up On Foil, Ep. 11 (podcast, 03.12.2024)",
    );
    b.body(
        "Pumpfoiling takes several hundred starts — 300 to 600 for most people. If no start \
counts as a failure, you cannot fail: every jump is simply the next repetition, and the \
repetitions are the sport. Everybody learns at their own speed, so the first thing I ask a new \
student is: what sports did you do as a kid? Skateboarding, gymnastics, dancing — any balance \
sport? Your background, height, weight and resilience shape your personal curve. Comparing \
yourself to anybody else is exactly the expectation we left at home.",
    );

    // ===================== 2. Why this sport ==============================
    b.h1("2. Why This Sport");
    b.body(
        "No engine — only your body. No wind, no waves, no boat needed: Switzerland has 1,500 \
lakes, and all you need is a board, a foil and a dock. Rain, snow, sunshine — there is no \
excuse. And it is a therapeutic sport, like a natural ADHD treatment:",
    );
    b.quote(
        "You have a problem in your marriage, with your kids, with some teachers — or you are \
just stressed out about your job. When you jump on the board, those three, four seconds, you \
need an autistic hyperfocus. It focuses you on the moment, on the instant you land on the \
board. You forget everything else. You completely reset your main brain.",
        "Pump Tsüri by Boardtales (video interview)",
    );
    b.body(
        "Once you touch the board, there is no more thinking — only muscle memory. The \
hyperfocus that is a burden in a classroom is a superpower on a foil: educate your autistic \
hyperfocus. Ambition lives in the future; the dockstart only accepts people who are fully in \
the present.",
    );
    b.quote(
        "In Zürich, when it rains, people are in a bad mood. It's like they are made of sugar \
or something. You go and pump, your bad mood disappears instantly.",
        "TotalSUP interview, 27.04.2023",
    );
    b.body(
        "I have two predictions: pump foiling will turn into the new national sport of \
Switzerland, and soon we will have more pump foilers in Switzerland than surfers in Hawaii.",
    );

    // ===================== 3. The progression =============================
    b.h1("3. The Progression: Belly, Knee, Feet");
    b.body(
        "This approach is meant for the broad mass of beginners — the exceptions come at the \
end. I start with all my beginners on the belly, no matter their age, size, weight or talent. \
Watch the videos (section 7) — they show this better than any text.\n\
\n\
1. Belly — about 20 to 100 tries. On your belly you are as close as possible to the foil's \
centre of gravity, so you experience the pure glide feeling with zero risk.\n\
\n\
2. Knee — again 20 to 100 tries. Push off with your jump leg, place the other knee on the \
board first, then bring the second knee up — and pump the board on your knees.\n\
\n\
3. The glide feeling. The student has to get the glide into their senses. Age plays a decisive \
role: you teach a 62-year-old differently than a 5-year-old — kids stand up from the knees \
much earlier.\n\
\n\
4. Ready. When the student glides more than 2 to 5 metres on the belly and on the knees, they \
have the glide feeling at a low centre of gravity. They are ready for the next step.\n\
\n\
5. Feet. When the student is ready for the feet, go to the feet — but the student must want \
it. When a kid says by herself or himself “I want to go on my feet”, she is mentally ready; \
then I hold the board. The hardest part of teaching is holding people back — they are like \
horses before the start. A detail that does half the work: teach on a board that is not too \
large. A smaller board disciplines the student — the feet have to go where they belong, \
otherwise you step off. If somebody insists on standing right away and then crashes and \
crashes, I say: go one step back, onto the belly, until you feel safe. The ones who understand \
stand up sooner.\n\
\n\
Exceptions always exist. With Andri Ragettli and David Hablützel, 3 belly tries and 3 knee \
tries were enough — then straight to the feet. And with impatient young students (16 to 25) \
you can go to the feet earlier, if they have good balance and they want it.",
    );

    // ===================== 4. Rules at the dock ===========================
    b.h1("4. Rules at the Dock");
    b.body(
        "1. Count starts, not sessions — plan for 300 to 600. Say it out loud on day one.\n\
2. Same spot, same board, same foil. A familiar setup removes variables; gear-hopping and \
spot-hopping reset your progress.\n\
3. Stabilizer in the most forward position on the fuselage. You want to get into the pump \
movement — steep down and up again — not just glide.\n\
4. Energy is not the answer. About 12 to 15 things have to be right at the same time — foot \
position, weight, body position. The ambitious ones cramp up, put too much energy in, and \
crash.\n\
5. Let your pulse come down between tries. From high pulse to high pulse there is no \
concentration.\n\
6. Silence during a start. Never talk to somebody who is starting — not even to cheer. \
Feedback waits until the person returns to the dock.\n\
7. Break the vicious cycle. Fail, get angry, curse, kick the board — and your temperament \
takes over and nothing works anymore. Stop, swim, breathe, reset.\n\
\n\
And once you can pump: I do a regular pump (left foot front), then a goofy pump (right foot \
front). What comes next is up to the rider — practice your own favourite tricks.",
    );

    // ===================== 5. Safety ======================================
    b.h1("5. Safety");
    b.body(
        "Of the hundreds of people I have taught, I have had one or two \
cutting accidents — both adults with too much ambition and too much energy in the start (one \
of them had a couple of beers first). “No ambitions, no expectations” is also the safety rule. \
With kids it is absolute: a kid who gets hurt once will not come back. And the beginner setup \
must float nose-up after a crash, so the board is easy to grab and easy on your back.",
    );

    // ===================== 6. The gear ====================================
    b.h1("6. The Gear");
    b.body(
        "I taught for years on the INDIANA 1100 P — a simple, sturdy, decently priced complete \
setup — and later on the 1190 and the 1396 (see Patrick's kneestart video below). Today I \
teach with the ONIX Pump Starter Pack: front wing 1850 cm² if you weigh up to 75 kg, 2250 cm² \
above that. Whatever you choose, choose once and stick with it. A starter set should not cost \
more than 1,500 francs, and you do not have to buy at all in the beginning: rent, or start \
together — two people, one board. If you both get addicted, you both buy your own. And the \
very first step costs nothing: speak to anybody who is out there trying it — walk up, ask \
where they rented the equipment and whether you could try once or twice. That is how this \
community works.",
    );
    b.break_(0.2);
    b.source_disp(
        "The ONIX Pump Starter Packs (1850 / 2250):",
        URL_ONIX_PACKS,
        URL_ONIX_PACKS,
    );

    // ===================== 7. Watch, then jump ============================
    b.h1("7. Watch, do not think before you start — Videos & Sources");
    b.body("Videos teach this sport better than text. Start here:");
    b.break_(0.2);
    b.source_disp(
        "Belly, Knee, Feet — the progression, with short videos of the kneestart (24.03.2025):",
        URL_BLOG_BKF,
        URL_BLOG_BKF,
    );
    b.source_disp(
        "A proper kneestart: 85 kg rider Patrick on the INDIANA 1396 (Instagram, 31.05.2026):",
        URL_IG_KNEESTART,
        URL_IG_KNEESTART,
    );
    b.source_disp(
        "Pump Tsüri by Boardtales — the Zürich pumpfoiling scene since 2020 (video):",
        URL_BOARDTALES,
        URL_BOARDTALES,
    );
    b.source_disp(
        "The Best Way to Learn Pump Foiling w/The Godfather — Up On Foil Ep. 11 (podcast):",
        URL_UPONFOIL,
        URL_UPONFOIL,
    );
    b.source_disp(
        "Can pump foiling become Switzerland's national sport? — interview (TotalSUP, 27.04.2023):",
        "totalsup.com → Can pump foiling become Switzerland's national sport?",
        URL_TOTALSUP,
    );
    b.source_disp(
        "Schulung — the Pump Tsüri training concept and weekly courses (Friday to Sunday):",
        URL_SCHULUNG,
        URL_SCHULUNG,
    );
    b.source_disp(
        "Pool Pump Zürich — indoor pumping every Friday for lunch, with video (SSA Riedtli, 30 °C water):",
        "pump.zuerich → Pool Pump Zürich every Friday for Lunch",
        URL_POOL,
    );
    b.source_disp(
        "Pumpfoil your ADHD — educate your autistic hyperfocus (24.04.2025):",
        URL_BLOG_ADHD,
        URL_BLOG_ADHD,
    );
    b.source_disp(
        "The therapeutic value of Pumpfoiling/Dockstarting (27.06.2023):",
        "pump.zuerich → The therapeutic value of Pumpfoiling/Dockstarting",
        URL_BLOG_THERAPY,
    );
    b.source_disp(
        "Pumpfoiling is good for you, specially if you are ADHD/ADD (Instagram, 23.04.2025):",
        URL_IG_ADHD,
        URL_IG_ADHD,
    );

    b.break_(0.6);
    b.line(
        "Remember: when you learn pumpfoiling — no ambitions, no expectations.",
        Style::new().with_color(ACCENT).with_font_size(10).italic(),
        Alignment::Left,
    );

    // ---- Render + links --------------------------------------------------
    std::fs::create_dir_all(OUT_DIR)?;
    let out = PathBuf::from(OUT_DIR).join(OUT_FILE);
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
    build(&font_dir)
}
