// Rechtsgrundlagen des Pumpfoilens auf dem Zürichsee — Begleitdokument zur
// Schriftlichen Anfrage GR Nr. 2026/250.
//
// Pure Rust, kein Chrome: das PDF wird direkt mit `genpdf` (über printpdf)
// erzeugt; die DejaVu-Sans-Familie wird eingebettet (deckt Latein, Umlaute und
// die «»„" Anführungszeichen ab). Die im Dokument zitierten Gesetzes- und
// Quellen-URLs werden anschliessend mit `lopdf` als anklickbare /Link-URI-
// Annotationen über die jeweilige URL-Zeile gelegt — genpdf 0.2 kann selbst
// keine Hyperlinks setzen.
//
//   cargo run --release --bin rechtsgrundlagen
//
// Font-Verzeichnis überschreiben: $FONT_DIR (Standard /usr/share/fonts/dejavu).

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use genpdf::elements::{Break, Paragraph};
use genpdf::style::{Color, Style};
use genpdf::Alignment;

const DEFAULT_FONT_DIR: &str = "/usr/share/fonts/dejavu";
const OUT_DIR: &str = "recht";
const OUT_FILE: &str = "Rechtsgrundlagen_Pumpfoiling_Zuerichsee.pdf";

// A4 Geometrie (Punkte) und Ränder, um die anklickbaren Rechtecke aufzuspannen.
const A4_WIDTH_PT: f64 = 595.276;
const MARGIN_MM: u8 = 20;

// URL-Zeilen sind die EINZIGEN Zeilen, die in dieser Schriftgrösse gesetzt
// werden. Daran erkennt add_links() sie im Content-Stream wieder (der Text
// selbst ist als CID kodiert, aber die Positions-Operatoren sind Klartext).
// Keine andere Textgrösse im Dokument darf 8.5 sein.
const LINK_FONT_SIZE: u8 = 9;

// Palette.
const INK: Color = Color::Rgb(0x1a, 0x1a, 0x1a);
const ACCENT: Color = Color::Rgb(0x0d, 0x47, 0x6b); // tiefes Seeblau
const GOLD: Color = Color::Rgb(0x9a, 0x7b, 0x2e);
const GREY: Color = Color::Rgb(0x55, 0x55, 0x55);
const LINKCOL: Color = Color::Rgb(0x12, 0x5a, 0x9c);
const QUOTECOL: Color = Color::Rgb(0x33, 0x33, 0x33);

// ---- Wichtige URLs (alle werden anklickbar gemacht) -----------------------
const URL_ANFRAGE: &str =
    "https://www.gemeinderat-zuerich.ch/geschaefte/detail.php?gid=8b364da34f9647b0b495d559ab43216c";
const URL_ANFRAGE_PDF: &str =
    "https://www.gemeinderat-zuerich.ch/dokumente/9c462ade300d409893076affcd766c97-332?filename=2026_0250SchriftlicheAnfrage";
const URL_PUMP: &str = "https://pump.zuerich";
const URL_BSG: &str = "https://www.fedlex.admin.ch/eli/cc/1976/725_724_724/de";
const URL_BSV: &str = "https://www.fedlex.admin.ch/eli/cc/1979/337_337_337/de";
const URL_BSV_ART37: &str = "https://www.fedlex.admin.ch/eli/cc/1979/337_337_337/de#art_37";
const URL_BSV_ART134A: &str = "https://www.fedlex.admin.ch/eli/cc/1979/337_337_337/de#art_134_a";
const URL_GSCHG: &str = "https://www.admin.ch/ch/d/sr/c814_20.html";
const URL_NHG: &str = "https://www.admin.ch/ch/d/sr/c451.html";
const URL_BGF: &str = "https://www.admin.ch/ch/d/sr/c923_0.html";
const URL_KONKORDAT: &str =
    "https://www.zh.ch/de/politik-staat/gesetze-beschluesse/gesetzessammlung/zhlex-ls/erlass-747_2-1979_10_04-1980_06_01-091.html";
const URL_WWG: &str =
    "https://www.zh.ch/de/politik-staat/gesetze-beschluesse/gesetzessammlung/zhlex-ls/erlass-724_11-1991_06_02-1993_01_01-099.html";
const URL_KT_SCHIFFFAHRT: &str = "https://www.zh.ch/de/mobilitaet/schifffahrt.html";
const URL_STADT_PUMPFOIL: &str =
    "https://www.stadt-zuerich.ch/de/stadtleben/sport-und-erholung/gewaesser/pumpfoiling.html";
const URL_STADT_SUP: &str =
    "https://www.stadt-zuerich.ch/de/stadtleben/sport-und-erholung/gewaesser/stand-up-paddling.html";

/// Sammelt das Dokument und merkt sich die Reihenfolge der URL-Zeilen, damit
/// add_links() sie hinterher den gefundenen Textpositionen zuordnen kann.
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

    /// Mehrzeiliger Fliesstext (jede `\n`-Zeile ein eigener Absatz).
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

    fn h2(&mut self, text: &str) {
        self.break_(0.5);
        self.line(text, Style::new().with_color(GOLD).with_font_size(12).bold(), Alignment::Left);
        self.break_(0.2);
    }

    /// Wörtliches Zitat (eingerückt, kursiv) mit Quellenangabe.
    fn quote(&mut self, text: &str, who: &str) {
        self.break_(0.2);
        for l in text.split('\n') {
            self.line(
                &format!("    «{}»", l.trim()),
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

    /// Eine Link-Zeile: `display` wird sichtbar in der reservierten Link-Schrift-
    /// grösse gesetzt (Marker für add_links), `url` wird als Ziel hinterlegt.
    /// genpdf wirft eine Zeile weg, wenn ein einzelnes, nicht umbrechbares Wort
    /// breiter als die Spalte ist — sehr lange URLs müssen daher als kürzerer
    /// `display`-Text gesetzt werden; der Klick führt trotzdem zur echten `url`.
    fn linkline(&mut self, display: &str, url: &str, align: Alignment) {
        self.line(display, Style::new().with_color(LINKCOL).with_font_size(LINK_FONT_SIZE), align);
        self.links.push(url.to_string());
    }

    /// Quellen-/Gesetzeszeile: Beschriftung als Fliesstext, darunter der Link.
    fn source(&mut self, label: &str, url: &str) {
        self.source_disp(label, url, url);
    }

    /// Wie `source`, aber mit eigenem (kurzem) Anzeigetext für den Link.
    fn source_disp(&mut self, label: &str, display: &str, url: &str) {
        self.line(label, Style::new().with_color(INK).with_font_size(10), Alignment::Left);
        self.linkline(display, url, Alignment::Left);
        self.break_(0.25);
    }

    /// Reiner Link ohne separate Beschriftungszeile.
    fn link(&mut self, url: &str) {
        self.linkline(url, url, Alignment::Left);
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
    doc.set_title("Rechtsgrundlagen des Pumpfoilens auf dem Zürichsee");
    doc.set_minimal_conformance();
    let mut deco = genpdf::SimplePageDecorator::new();
    deco.set_margins(MARGIN_MM);
    doc.set_page_decorator(deco);

    let mut b = Builder { doc, links: Vec::new() };

    // ===================== Titel ==========================================
    b.break_(3.0);
    b.line(
        "RECHTSGRUNDLAGEN",
        Style::new().with_color(GOLD).with_font_size(12).bold(),
        Alignment::Center,
    );
    b.break_(1.2);
    b.line(
        "Pumpfoilen auf dem Zürichsee",
        Style::new().with_color(INK).with_font_size(24).bold(),
        Alignment::Center,
    );
    b.break_(1.0);
    b.line(
        "Die anwendbaren Gesetze, Verordnungen und Zuständigkeiten",
        Style::new().with_color(ACCENT).with_font_size(13).italic(),
        Alignment::Center,
    );
    b.break_(1.6);
    b.line(
        "Begleitdokument zur Schriftlichen Anfrage GR Nr. 2026/250",
        Style::new().with_color(GREY).with_font_size(11),
        Alignment::Center,
    );
    b.line(
        "«Pumpfoiling am Zürichsee» (N. Eggenschwiler, M. Denoth, F. Blättler, SP) — 27.05.2026",
        Style::new().with_color(GREY).with_font_size(10),
        Alignment::Center,
    );
    b.break_(1.2);
    // Zentrierte Quelllinks zum Geschäft (anklickbar).
    b.line(
        "Geschäft im Gemeinderat Zürich:",
        Style::new().with_color(INK).with_font_size(10),
        Alignment::Center,
    );
    // Auf der Titelseite zentriert; add_links spannt das Rechteck über die
    // ganze Zeilenbreite, daher funktioniert die Erkennung auch zentriert.
    b.line(URL_ANFRAGE, Style::new().with_color(LINKCOL).with_font_size(LINK_FONT_SIZE), Alignment::Center);
    b.links.push(URL_ANFRAGE.to_string());
    b.break_(0.3);
    b.line(
        "Schriftliche Anfrage (PDF):",
        Style::new().with_color(INK).with_font_size(10),
        Alignment::Center,
    );
    b.linkline(
        "gemeinderat-zuerich.ch → 2026_0250 Schriftliche Anfrage (PDF)",
        URL_ANFRAGE_PDF,
        Alignment::Center,
    );

    b.break_(2.0);
    b.line(
        &format!("Stand: {} · Zusammengestellt für Pump Tsüri / Pump Foil Zürichsee", today()),
        Style::new().with_color(GREY).with_font_size(8),
        Alignment::Center,
    );

    b.doc.push(genpdf::elements::PageBreak::new());

    // ===================== 1. Anlass ======================================
    b.h1("1. Anlass und Ausgangslage");
    b.body(
        "Pumpfoiling — die Fortbewegung auf einem Unterwassertragflügel mittels Pumpbewegungen — \
ist auf dem Zürichsee zu einer rasch wachsenden, nicht motorisierten Wassersportart geworden. \
Als nicht motorisierter Wassersport verursacht Pumpfoilen weder Lärm- noch Schadstoffemissionen.",
    );
    b.break_(0.4);
    b.body(
        "Für Anfänger*innen sind feste Stege oder Flosse zentral: Die ersten Trainingsschritte \
finden auf engem Raum statt, und der sichere Start ab einem Floss lässt sich dort besonders \
effizient erlernen. In den Jahren 2021–2025 führten das Sportamt, der ASVZ und Pump Tsüri \
erfolgreich Pumpfoil-Kurse auf den Flossen städtischer Badeanstalten ausserhalb der \
Badeöffnungszeiten durch (gegen Miete). Diese Bewilligung wurde nach Abklärungen durch das \
AWEL unter Verweis auf die geltenden Regelungen für die gelb markierten Sperrzonen widerrufen.",
    );
    b.break_(0.4);
    b.body(
        "Die Schriftliche Anfrage GR Nr. 2026/250 stellt dem Stadtrat dazu acht Fragen. Das \
vorliegende Dokument stellt die tatsächlich anwendbaren Rechtsgrundlagen zusammen — vom Bund \
über das interkantonale Recht bis zu Kanton und Stadt — und dokumentiert, worauf sich die \
Behörden im konkreten Fall berufen haben.",
    );
    b.break_(0.4);
    b.source("Schriftliche Anfrage GR Nr. 2026/250 — Geschäftsseite:", URL_ANFRAGE);
    b.source_disp(
        "Schriftliche Anfrage GR Nr. 2026/250 — Volltext (PDF):",
        "gemeinderat-zuerich.ch → 2026_0250 Schriftliche Anfrage (PDF)",
        URL_ANFRAGE_PDF,
    );
    b.source("Hintergrund / Dokumentation Pump Tsüri:", URL_PUMP);

    // ===================== 2. Rechtliche Einordnung =======================
    b.h1("2. Rechtliche Einordnung des Pumpfoils");
    b.body(
        "Massgeblich ist zunächst die Frage, was ein Pumpfoil rechtlich ist. Die Behörden \
(Schifffahrtskontrolle und Wasserschutzpolizei) ordnen das Pumpfoil schweizweit einheitlich \
als Schiff im Sinne der Binnenschifffahrtsverordnung (BSV) ein, und zwar als Unterkategorie \
«wettkampftaugliche Wassersportgeräte» (Art. 134a BSV) — vergleichbar mit Kitesurf- und \
Segelbrettern, Stand-up-Paddle-Boards und ähnlichen Geräten.",
    );
    b.quote(
        "Gemäss Binnenschifffahrtsverordnung gelten Pumpfoils als Schiffe, Unterkategorie «wettkampftaugliche Wassersportgeräte». Dies wird schweizweit einheitlich so durch alle Schifffahrtsämter und Wasserpolizeidienststellen gehandhabt. Die gelben Sperrflächen gelten für alle Schiffe.",
        "Daniel Mattille, Wasserschutzpolizei Stadt Zürich (E-Mail, Juli 2025)",
    );
    b.body(
        "Aus dieser Einordnung folgt die gesamte Anwendbarkeit des Schifffahrtsrechts: Verhalten, \
Vortritt, Sperrgebiete, Rettungsmittel und Uferzonen. Als «wettkampftaugliches Wassersportgerät» \
ist das Pumpfoil innerhalb der inneren Uferzone von der Pflicht zum Mitführen von Rettungsmitteln \
befreit; ausserhalb der äusseren Uferzone (mehr als 300 m vom Ufer) ist eine Schwimmhilfe nach \
Norm SN EN ISO 12402-5 mitzuführen (Art. 134/134a BSV). Pumpfoils sind nicht immatrikulierbar, \
sind aber mit Name und Adresse der Halterin/des Halters zu beschriften (Art. 16 BSV).",
    );
    b.source("BSV — Art. 134a (wettkampftaugliche Wassersportgeräte):", URL_BSV_ART134A);

    // ===================== 3. Bundesrecht =================================
    b.h1("3. Bundesrecht");

    b.h2("3.1 Binnenschifffahrt");
    b.body(
        "Bundesgesetz über die Binnenschifffahrt (BSG) vom 3. Oktober 1975, SR 747.201 — \
Grundlage der gesamten Schifffahrt auf schweizerischen Gewässern.",
    );
    b.link(URL_BSG);
    b.break_(0.25);
    b.body(
        "Verordnung über die Schifffahrt auf schweizerischen Gewässern (Binnenschifffahrts-\
verordnung, BSV) vom 8. November 1978, SR 747.201.1 — das zentrale, im konkreten Fall \
einzig angerufene Regelwerk. Einschlägig sind insbesondere:",
    );
    b.body(
        "• Art. 37 — Schwimmkörper / Bezeichnung von Sperrflächen (gelbe Bojen)\n\
• Art. 53 i.V.m. Anhang 4 — Befahrungsverbot in markierten Sperrgebieten (Badeanlagen, \
archäologische Schutzzonen, Naturschutzgebiete) sowie Schilf-, Binsen- und Seerosenzonen\n\
• Art. 25 — Lichterführung bei schlechter Sicht\n\
• Art. 40 — Sturm- und Windwarnung\n\
• Art. 43–44 — Vortritt (Kurs- und Güterschiffe, Fischer, Segelschiffe haben Vortritt)\n\
• Art. 121 — Verbot der Motorisierung von Wassersportgeräten\n\
• Art. 134 / 134a — Rettungsmittel, äussere Uferzone (300 m), wettkampftaugliche Geräte",
    );
    b.source("BSV — Volltext (fedlex):", URL_BSV);
    b.source("BSV — Art. 37 (Schwimmkörper / Sperrflächen):", URL_BSV_ART37);
    b.break_(0.2);
    b.body(
        "Hinweis: Das BSG stammt aus dem Jahr 1975. Eine grundsätzliche Öffnung der Sperrflächen \
für das Pumpfoilen wäre nach Auffassung der Behörden nur über eine Anpassung der BSV auf \
Bundesebene möglich (siehe Abschnitt 7).",
    );

    b.h2("3.2 Gewässer- und Naturschutz");
    b.body(
        "Bundesgesetz über den Schutz der Gewässer (Gewässerschutzgesetz, GSchG) vom 24. Januar 1991, \
SR 814.20, mit Gewässerschutzverordnung (GSchV, SR 814.201) — Gewässerraum, Schutz von \
Ufervegetation und Lebensräumen.",
    );
    b.link(URL_GSCHG);
    b.break_(0.25);
    b.body(
        "Bundesgesetz über den Natur- und Heimatschutz (NHG) vom 1. Juli 1966, SR 451 — \
Schutz von Ufer- und Wasservogel-Lebensräumen; Grundlage zahlreicher Schutzgebiete am Seeufer.",
    );
    b.link(URL_NHG);
    b.break_(0.25);
    b.body(
        "Bundesgesetz über die Fischerei (BGF) vom 21. Juni 1991, SR 923.0 — Schutz der Uferzonen, \
Laich- und Schongebiete, die teils mit den Sperrflächen zusammenfallen.",
    );
    b.link(URL_BGF);

    // ===================== 4. Interkantonales Recht =======================
    b.h1("4. Interkantonales Recht");
    b.body(
        "Interkantonale Vereinbarung über die Schifffahrt auf dem Zürichsee und dem Walensee \
vom 4. Oktober 1979 (in Kraft seit 1. Juni 1980), LS 747.2. Die Uferkantone Zürich, Schwyz, \
Glarus und St. Gallen regeln darin gemeinsam die Schifffahrt auf den geteilten Gewässern \
(u.a. Bezeichnungen, Sturmwarnung, Seerettungsdienst).",
    );
    b.source_disp(
        "Interkantonale Vereinbarung Zürichsee/Walensee (LS 747.2):",
        "zh.ch → Gesetzessammlung (zhlex), LS 747.2",
        URL_KONKORDAT,
    );

    // ===================== 5. Kantonales Recht ============================
    b.h1("5. Kantonales Recht (Zürich)");
    b.body(
        "Auf kantonaler Ebene gelten ergänzend zum Bundesrecht insbesondere:",
    );
    b.body(
        "• Einführungsgesetz zum Bundesgesetz über die Binnenschifffahrt — LS 747.1\n\
• Schifffahrtsverordnung — LS 747.11\n\
• Schiffsstationierungsverordnung (SchSV) — LS 747.4",
    );
    b.break_(0.25);
    b.body(
        "Zuständig für Schiffe und Sperrflächen ist das Strassenverkehrsamt (Schifffahrtskontrolle); \
die Ufer- und Gewässernutzung sowie die Sperrflächen betreut das AWEL (Amt für Abfall, Wasser, \
Energie und Luft, Baudirektion). Die «Ufer-, Sperr- und Sportverbotszonen» für Zürichsee und \
Walensee sind in der App «Auf Kurs» und auf den amtlichen Karten verzeichnet.",
    );
    b.source("Kanton Zürich — Schifffahrt (Übersicht, Zuständigkeiten):", URL_KT_SCHIFFFAHRT);

    b.h2("5.1 Wasserwirtschafts-/Wassergesetz");
    b.body(
        "Wasserwirtschaftsgesetz (WWG) vom 2. Juni 1991, LS 724.11 — Wasserrechte, Wasserpolizei, \
Nutzung der öffentlichen Gewässer (u.a. Mindestabstände zu Gewässern).",
    );
    b.source_disp(
        "Wasserwirtschaftsgesetz WWG (LS 724.11):",
        "zh.ch → Gesetzessammlung (zhlex), LS 724.11",
        URL_WWG,
    );
    b.break_(0.2);
    b.body(
        "Neues Wassergesetz (WsG): Das WWG wird durch ein neues kantonales Wassergesetz abgelöst. \
Dessen Inkraftsetzung wurde wegen einer (inzwischen zurückgezogenen) Einsprache verschoben und \
ist neu auf den 1. Juni 2026 geplant. Mit dem neuen Gesetz werden gewerbliche Nutzungen \
öffentlicher Gewässer bewilligungspflichtig — dies betrifft jedoch die Nutzung ausserhalb der \
Sperrflächen und eröffnet keinen Zugang innerhalb der gelben Zone.",
    );
    b.quote(
        "Zu beachten ist, dass voraussichtlich per 1. November 2025 ein neues Wassergesetz in Kraft treten wird, mit dem jegliche gewerblichen Nutzungen auf öffentlichen Gewässern bewilligungspflichtig werden. […] Das sollte aber (ausserhalb der Sperrzone) nicht kritisch sein.",
        "Fabienne Mouret, AWEL (E-Mail, 3. Juli 2025)",
    );
    b.quote(
        "Auf Grund einer Einsprache hat sich die Einführung des neuen Wassergesetzes verzögert. Inzwischen wurde die Einsprache aber zurückgezogen und die Einführung des Gesetzes ist neu auf den 1. Juni 2026 geplant.",
        "Fabienne Mouret, AWEL (E-Mail, 27. Februar 2026)",
    );

    b.h2("5.2 Natur- und Uferschutz");
    b.body(
        "Kantonales Natur- und Heimatschutzrecht sowie Schutzverordnungen am Seeufer (Inventar der \
Natur- und Landschaftsschutzobjekte; Fachstelle Naturschutz) konkretisieren die geschützten \
Uferzonen. Der Seeuferschutz wird zunehmend mit Mitteln der Raumplanung, des Natur- und \
Heimatschutzes sowie des Gewässerschutzes (Gewässerraum) gesteuert.",
    );

    // ===================== 6. Städtisches Recht ===========================
    b.h1("6. Städtisches Recht (Stadt Zürich)");
    b.body(
        "Benutzungsordnung für Sport- und Badeanlagen (BO SBA) vom 5. November 2024 (AS 421.150) \
sowie die Badeordnung der öffentlichen Badeanlagen — sie regeln die Nutzung der Badeanlagen und \
ihrer Flosse sowie die Badeöffnungszeiten. Die Vermietung/Bewilligung erfolgt durch das Sportamt \
(Bade- und Eisanlagen).",
    );
    b.body(
        "Allgemeine Polizeiverordnung der Stadt Zürich (APV) — allgemeines Verhalten im öffentlichen \
Raum. Für die konkrete Kursdurchführung relevant war die Saisonbewilligung des Sportamts \
(Gesuch 6783, «Saisonbewilligung Tiefenbrunnen — Pumpfoil — Saison 2025»), die am 10. Juni 2025 \
erteilt und am 1. Juli 2025 widerrufen wurde.",
    );
    b.source("Stadt Zürich — Pumpfoiling (gesetzliche Grundlagen):", URL_STADT_PUMPFOIL);
    b.source("Stadt Zürich — Stand-up-Paddling (Regeln, Sperrgebiete):", URL_STADT_SUP);

    // ===================== 7. Gelbe Zone und Ausnahmefrage ================
    b.h1("7. Die «gelbe Zone» (Sperrfläche) und die Ausnahmefrage");
    b.body(
        "Die «gelbe Zone» ist die durch gelbe Bojen abgegrenzte Sperrfläche (Schwimmer-/Sperrzone der \
Badeanstalten). Rechtlich ist sie eine Sperrfläche nach BSV (Art. 37 i.V.m. Art. 53 / Anhang 4); \
da das Pumpfoil als Schiff gilt, gilt das Befahrungsverbot der Sperrfläche auch für Pumpfoils — \
nach Auffassung der Behörden unabhängig von Tageszeit und Badesaison.",
    );

    b.h2("7.1 Die ablehnende Position");
    b.quote(
        "Leider ist das Befahren von solchen Sperrflächen, egal zu welcher Uhrzeit, per Definition nicht erlaubt. Es gibt hier auch keine gesetzlichen Möglichkeiten für Ausnahmen.",
        "Fabienne Mouret, AWEL (E-Mail, Juli 2025)",
    );
    b.quote(
        "Eine Nutzung von Pumpfoils innerhalb der gelben Sperrflächen (egal ob in oder ausserhalb der Badeanlagen oder deren Öffnungszeiten) ist nicht zulässig. Eine «Öffnung» für Schiffe, egal welcher Kategorie, wird durch uns nicht unterstützt.",
        "Daniel Mattille, Wasserschutzpolizei (E-Mail, 4. Juli 2025)",
    );

    b.h2("7.2 AWEL hält eine Ausnahme jedoch ausdrücklich für rechtlich möglich");
    b.body(
        "Wichtig — und in scheinbarem Widerspruch zur obigen Aussage: Der zuständige Gebietsbetreuer \
des AWEL hat schriftlich festgehalten, dass eine temporäre Aufhebung der Sperrfläche für das \
Zeitfenster 7–9 Uhr rechtlich möglich wäre:",
    );
    b.quote(
        "Eine temporäre Aufhebung der Sperrfläche (z.B. von 7-9 Uhr) wäre zwar möglicherweise rechtlich möglich, aber aus diversen Gründen nicht wirklich sinnvoll (z.B. könnte während dieser Phase die Fläche auch durch div. andere Nutzungsarten belegt werden, Schwierigkeiten mit der Gleichbehandlung, usw.).",
        "David Huber, AWEL, Gebietsbetreuer Stadt Zürich (E-Mail, 3. Juli 2025)",
    );
    b.body(
        "Das AWEL skizziert zudem einen konkreten Weg zu einer Bewilligung — über die Unterstützung \
des Sportamts und eine Prüfung in der «Drehscheibe Wasser»:",
    );
    b.quote(
        "Sofern das Sportamt das Vorhaben unterstützen möchte, könnte ein Abklärungs- bzw. Planungsprozess gestartet werden, welcher eine nachhaltige Lösung für die Nutzung anstrebt […]. Diesbezügliche Bemühungen könnten in der Drehscheibe Wasser auf Ihre Bewilligungsfähigkeit hin überprüft werden.",
        "David Huber, AWEL, Gebietsbetreuer Stadt Zürich (E-Mail, 3. Juli 2025)",
    );
    b.body(
        "Damit steht die pauschale Aussage «keine gesetzlichen Möglichkeiten für Ausnahmen» im \
Widerspruch zur fachlichen Einschätzung des AWEL-Gebietsbetreuers, wonach eine zeitlich \
befristete Aufhebung der Sperrfläche rechtlich möglich und über einen Planungsprozess \
bewilligungsfähig wäre. Genau hier setzt die Schriftliche Anfrage GR Nr. 2026/250 an: Sie fragt \
nach einer Ausnahmeregelung für die Nutzung der Flosse zu Schulungszwecken ausserhalb der \
Badeöffnungszeiten.",
    );

    b.h2("7.3 Sachargument: Pumpfoil im Vergleich zum Stand-up-Paddle");
    b.body(
        "Für die Verhältnismässigkeit relevant ist der Volumenvergleich: Ein Pumpfoilbrett hat \
typischerweise nur 5–15 Liter Volumen — vergleichbar mit einem Rettungsring —, während ein \
Stand-up-Paddle-Brett meist über 150 Liter Volumen aufweist. Rettungs-SUP bewegen sich \
während des Badebetriebs regulär innerhalb der Sperrzone. Eine sichere, geordnete und \
umweltverträgliche Schulung von 7–9 Uhr vor dem Badebetrieb erscheint vor diesem Hintergrund \
gut begründbar.",
    );

    b.h2("7.4 Präzedenzfall Greifensee: kein Rechtsgrund für ein Verbotsschild");
    b.body(
        "Ein aufschlussreicher Vergleichsfall stammt vom Greifensee. Dort konnte ein Ruderclub \
nicht verhindern, dass sein Steg zum Üben des Pumpfoilens genutzt wird. Nach einer Reklamation \
schaltete sich das AWEL ein und stellte kurzerhand eine Verbotstafel auf.",
    );
    b.body(
        "Der Rechtsdienst des Kantons hielt jedoch fest, dass es für ein solches Verbotsschild — \
gleich welcher Art — keinerlei gesetzliche Grundlage gibt: Der Steg steht jedermann ohne \
Einschränkung offen. Auch die Konzession räumt dem Ruderclub keinerlei Benutzungspriorität ein.",
    );
    b.quote(
        "Jedermann, keinerlei Einschränkung. Es gibt gemäss Rechtsdienst des Kantons keinerlei Grundlage für ein Verbotsschild irgendwelcher Art. In der Konzession steht auch nicht, dass der Ruderclub irgendeine Benutzungspriorität hat.",
        "Dokumentierter Fall Greifensee (Rechtsdienst des Kantons Zürich)",
    );
    b.body(
        "Dokumentarisch belegt wird dies durch die Verfügung des AWEL vom 19. Mai 2017 \
(Ref. AWEL 17-0128, gez. Christoph Noll, Sektionsleiter Wasserbau/Gewässernutzung), mit der dem \
Ruderclub Greifensee die wasserrechtliche Konzession für den Ersatz seines Rudersteges \
(Bootsplatz Nr. 27, Stationierungsanlage Greifensee) erteilt wurde. Sie hält als verbindliche \
Nebenbestimmung (Ziff. III.3) ausdrücklich fest:",
    );
    b.quote(
        "Der Steg muss allen Nutzenden zur Verfügung stehen.",
        "AWEL, Verfügung vom 19.5.2017 (Ref. AWEL 17-0128), Ziff. III.3",
    );
    b.body(
        "Schon in den Erwägungen hielt das AWEL fest, der Steg werde zwar insbesondere zum \
Einwassern von Rennruderbooten benützt, «er kann jedoch von allen Seenutzern benützt werden. Das \
Vorhaben liegt im öffentlichen Interesse.» Die Konzession begründet damit gerade keine \
Ausschliesslichkeit oder Benutzungspriorität des Ruderclubs — im Gegenteil ist die Nutzung durch \
alle eine behördlich verfügte Bedingung des Steg-Fortbestands. (Die Verfügung liegt diesem \
Dossier als Quelle bei.)",
    );
    b.body(
        "Der Fall zeigt exemplarisch, dass eine behördlich aufgestellte Verbotstafel für sich \
allein noch keine Rechtsgrundlage schafft — massgeblich ist, ob das zugrunde liegende Recht \
(Konzession, Verordnung, Sperrflächenregelung) das Verbot tatsächlich trägt. Er unterstreicht \
die Notwendigkeit, im Zürichsee-Fall die konkrete rechtliche Grundlage jeder Nutzungs- oder \
Zugangsbeschränkung offenzulegen.",
    );

    // ===================== 8. Zuständige Stellen ==========================
    b.h1("8. Zuständige Stellen");
    b.body(
        "Bund — Bundesamt für Verkehr (BAV): Freizeitschifffahrt, Vorschriften BSG/BSV.\n\
Kanton Zürich — AWEL, Baudirektion (Wasserbau, Ufer- und Gewässernutzung, Seen): Sperrflächen, \
Gewässernutzung. — Strassenverkehrsamt, Schifffahrtskontrolle (Oberrieden): Schiffe, Zonen.\n\
Stadt Zürich — Stadtpolizei, Wasserschutzpolizei (Mythenquai): Durchsetzung auf dem See. — \
Sportamt, Bade- und Eisanlagen: Badeanlagen, Flosse, Bewilligungen.",
    );
    b.break_(0.4);
    b.line(
        "Dieses Dokument fasst die Rechtslage zusammen; massgeblich sind die jeweils geltenden \
Gesetzestexte in ihrer aktuellen Fassung.",
        Style::new().with_color(GREY).with_font_size(8).italic(),
        Alignment::Left,
    );

    // ---- Render + Links --------------------------------------------------
    std::fs::create_dir_all(OUT_DIR)?;
    let out = PathBuf::from(OUT_DIR).join(OUT_FILE);
    let links = b.links.clone();
    b.doc.render_to_file(&out).map_err(|e| anyhow!("render {}: {}", out.display(), e))?;
    let n = add_links(&out, &links)?;
    eprintln!("wrote {} ({} anklickbare Links)", out.display(), n);
    Ok(())
}

fn today() -> String {
    use chrono::Local;
    Local::now().format("%d.%m.%Y").to_string()
}

/// Legt anklickbare /Link-URI-Annotationen über alle URL-Zeilen. genpdf 0.2
/// kann keine Hyperlinks; daher wird das fertige PDF mit lopdf wieder geöffnet,
/// jede in LINK_FONT_SIZE gesetzte Textzeile über alle Seiten (in Lesereihen-
/// folge) lokalisiert und der Reihe nach den gesammelten URLs zugeordnet. Das
/// Rechteck spannt die ganze Zeilenbreite (linker bis rechter Rand), damit es
/// auch bei links- wie zentriert gesetzten Zeilen passt.
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

    // Über alle Seiten in Seitenreihenfolge die Ursprünge der Link-Zeilen
    // sammeln: (page_id, y).
    let mut hits: Vec<(lopdf::ObjectId, f64)> = Vec::new();
    let pages = doc.get_pages(); // BTreeMap<u32, ObjectId> — nach Seitennr. sortiert
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
            "  Warnung: {} Link-Zeilen erkannt, aber {} URLs erfasst — Zuordnung ggf. unvollständig.",
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
