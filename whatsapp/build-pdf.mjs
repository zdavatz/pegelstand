#!/usr/bin/env node
// Build an HTML transcript from messages.json (text + inline image attachments),
// list non-image attachments, then render to PDF with weasyprint.
//
// Usage: node build-pdf.mjs <dir> <outHtml>

import { readFileSync, existsSync } from "fs";
import { resolve, extname } from "path";

const DIR = resolve(process.argv[2] || "/tmp/erica_wa");
const OUT = resolve(process.argv[3] || "/tmp/erica_wa/transcript.html");
const data = JSON.parse(readFileSync(resolve(DIR, "messages.json"), "utf8"));

const esc = (s) => String(s || "").replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
const IMG = new Set([".jpg", ".jpeg", ".png", ".webp", ".gif"]);
const fmt = (iso) => {
  const d = new Date(iso);
  const p = (n) => String(n).padStart(2, "0");
  return `${p(d.getDate())}.${p(d.getMonth() + 1)}.${d.getFullYear()} ${p(d.getHours())}:${p(d.getMinutes())}`;
};

let rows = "";
const m = data.matches[0] || { messages: [], name: "" };
for (const msg of m.messages) {
  const who = msg.fromMe ? "Zeno (ich)" : (msg.sender || m.name || "Erica Baumann");
  const cls = msg.fromMe ? "me" : "them";
  let body = "";
  if (msg.file && existsSync(resolve(DIR, msg.file))) {
    const ext = extname(msg.file).toLowerCase();
    if (IMG.has(ext)) body += `<div class="att"><img src="${esc(msg.file)}"/></div>`;
    else body += `<div class="doc">📎 Anhang: ${esc(msg.file)} (${msg.type}${msg.bytes ? ", " + Math.round(msg.bytes / 1024) + " KB" : ""})</div>`;
  } else if (msg.type !== "text") {
    body += `<div class="doc">[${esc(msg.type)}${msg.downloadError ? " — Download fehlgeschlagen" : ""}]</div>`;
  }
  if (msg.text) body += `<div class="txt">${esc(msg.text)}</div>`;
  if (!body) body = `<div class="txt"><em>[leer]</em></div>`;
  rows += `<div class="msg ${cls}"><div class="meta">${esc(who)} · ${fmt(msg.iso)}</div>${body}</div>\n`;
}

const html = `<!doctype html><html><head><meta charset="utf-8"><style>
@page { size: A4; margin: 18mm 15mm; }
body { font-family: "DejaVu Sans", Arial, sans-serif; font-size: 11pt; color: #111; }
h1 { font-size: 16pt; margin: 0 0 2mm; }
.sub { color: #555; font-size: 10pt; margin-bottom: 6mm; }
.msg { margin: 0 0 4mm; padding: 2mm 3mm; border-radius: 3mm; max-width: 85%; }
.them { background: #f0f0f0; }
.me { background: #d9fdd3; margin-left: auto; }
.meta { font-size: 8.5pt; color: #555; margin-bottom: 1mm; }
.txt { white-space: pre-wrap; word-wrap: break-word; }
.att img { max-width: 100%; max-height: 150mm; border-radius: 2mm; margin: 1mm 0; }
.doc { font-style: italic; color: #333; background:#fff3cd; padding:1mm 2mm; border-radius:2mm; }
</style></head><body>
<h1>WhatsApp – ${esc(m.name || "Erika Baumann")}</h1>
<div class="sub">Nachrichten seit ${esc(fmt(new Date(data.cutoff * 1000).toISOString()))} · ${m.messages.length} Nachrichten · erstellt für Zeno Davatz</div>
${rows}
</body></html>`;

import { writeFileSync } from "fs";
writeFileSync(OUT, html);
console.log(`HTML written: ${OUT} (${m.messages.length} messages)`);
