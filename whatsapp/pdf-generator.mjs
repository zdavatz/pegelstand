#!/usr/bin/env node
// Render messages.json into a "Baugeschichte" (building history) document:
// a titled photo-documentation of the house in Ermioni, NOT a chat transcript.
//
// Usage: node pdf-generator.mjs <dir> <outHtml>

import { readFileSync, writeFileSync, existsSync } from "fs";
import { resolve, extname } from "path";

const DIR = resolve(process.argv[2] || "/tmp/erica_wa");
const OUT = resolve(process.argv[3] || "/tmp/erica_wa/baugeschichte.html");
const data = JSON.parse(readFileSync(resolve(DIR, "messages.json"), "utf8"));
const m = data.matches[0] || { messages: [], name: "Erica Baumann" };

const esc = (s) => String(s || "").replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
const IMG = new Set([".jpg", ".jpeg", ".png", ".webp", ".gif"]);

// Erica's own messages only, in chronological order.
const hers = m.messages.filter((x) => !x.fromMe).sort((a, b) => a.ts - b.ts);
const photos = hers.filter((x) => x.file && IMG.has(extname(x.file).toLowerCase()));
// Text-only notes from her, excluding the opening message and the housekeeping note.
const textNotes = hers.filter((x) => !x.file && x.text && x.text.trim());

// First text = the introductory greeting/context; treat separately.
const intro = textNotes[0]?.text || "";
// Closing remarks (she announces more photos / a break) -> a soft note at the end.
const closing = textNotes.slice(1).map((x) => x.text.trim());

const figures = photos
  .map((p, i) => {
    const cap = (p.text || "").trim();
    return `<figure class="plate">
  <div class="num">Bild ${i + 1}</div>
  <img src="${esc(p.file)}"/>
  ${cap ? `<figcaption>${esc(cap).replace(/\n/g, "<br/>")}</figcaption>` : ""}
</figure>`;
  })
  .join("\n");

const closingHtml = closing.length
  ? `<div class="note"><p>${closing.map(esc).map((t) => t.replace(/\n/g, "<br/>")).join("</p><p>")}</p>
     <p class="sig">— ${esc(m.name || "Erica Baumann")}</p></div>`
  : "";

const html = `<!doctype html><html lang="de"><head><meta charset="utf-8"><style>
@page { size: A4; margin: 20mm 18mm 18mm; }
@page :first { margin: 0; }
* { box-sizing: border-box; }
body { font-family: "Georgia", "DejaVu Serif", serif; color: #1f1a16; line-height: 1.5; }

/* Cover */
.cover { height: 297mm; width: 210mm; padding: 40mm 28mm; display: flex; flex-direction: column;
  justify-content: center; background: #2f2a25; color: #f5efe6; page-break-after: always; }
.cover .kicker { letter-spacing: 4px; text-transform: uppercase; font-size: 11pt; color: #cbb894; margin-bottom: 8mm; }
.cover h1 { font-size: 34pt; line-height: 1.15; margin: 0 0 6mm; font-weight: 700; }
.cover .place { font-size: 15pt; font-style: italic; }
.cover .place a { color: #e3c98e; text-decoration: underline; }
.cover .rule { width: 40mm; height: 2px; background: #cbb894; margin: 12mm 0; }
.cover .lead { font-size: 12pt; color: #e7ded2; max-width: 120mm; }

/* Intro */
.intro { margin: 0 0 12mm; }
.intro h2 { font-size: 15pt; color: #6b5a44; border-bottom: 1px solid #d9cdbb; padding-bottom: 2mm; margin: 0 0 5mm; }
.intro blockquote { margin: 0; padding: 4mm 6mm; border-left: 3px solid #cbb894; background: #faf6ef;
  font-style: italic; white-space: pre-wrap; font-size: 11.5pt; }

/* Photo plates */
.plate { margin: 0 0 12mm; page-break-inside: avoid; text-align: center; }
.plate .num { font-size: 9.5pt; letter-spacing: 3px; text-transform: uppercase; color: #a08b6a; margin-bottom: 3mm; }
.plate img { max-width: 100%; max-height: 165mm; border: 1px solid #ddd; box-shadow: 0 1mm 4mm rgba(0,0,0,.18); }
.plate figcaption { margin: 4mm auto 0; max-width: 150mm; font-size: 11.5pt; color: #2f2a25;
  background: #f3ede3; padding: 3mm 5mm; border-radius: 1mm; text-align: left; }

/* Closing note */
.note { margin-top: 10mm; padding: 5mm 6mm; background: #faf6ef; border: 1px solid #e4d9c6; border-radius: 1mm;
  font-size: 11pt; color: #4a4035; }
.note .sig { text-align: right; font-style: italic; color: #6b5a44; margin-top: 4mm; }
.footer { margin-top: 14mm; text-align: center; font-size: 8.5pt; color: #9a8e7d; }
</style></head><body>

<section class="cover">
  <div class="kicker">Baugeschichte in Bildern</div>
  <h1>Das Haus von<br/>${esc(m.name || "Erica Baumann")}</h1>
  <div class="place"><a href="https://www.google.com/maps/search/?api=1&amp;query=Ermioni%2C+Griechenland">Ermioni, Griechenland</a></div>
  <div class="rule"></div>
  <div class="lead">Vom Kauf im Jahr 1990 bis zu den späteren Umbauten — eine fotografische Dokumentation der Veränderungen über die Jahre.</div>
</section>

${intro ? `<section class="intro">
  <h2>Zur Geschichte des Hauses</h2>
  <blockquote>${esc(intro)}</blockquote>
</section>` : ""}

${figures}

${closingHtml}

<div class="footer">Zusammengestellt aus den Bildern und Beschreibungen von ${esc(m.name || "Erica Baumann")} · Haus in Ermioni</div>
</body></html>`;

writeFileSync(OUT, html);
console.log(`HTML written: ${OUT} — ${photos.length} Bilder, intro=${intro ? "yes" : "no"}, closing=${closing.length}`);
