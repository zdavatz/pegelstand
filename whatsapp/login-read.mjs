#!/usr/bin/env node
// Login + read in ONE continuous session. The 2-step flow (login-qr.mjs then
// reader-merge.mjs) fails because the *second* process re-connecting with the
// freshly-paired creds gets force-unlinked by WhatsApp (401) within ~2s. Here
// the same socket that survives the pairing (incl. the 515 restart) keeps
// running and collects the chat — no second connection, no unlink.
//
// Usage: node login-read.mjs <nameFilter> <cutoffUnixSeconds> [waitSeconds] [outDir]
//   env NUM = digit-only number filter (e.g. 41765073911)

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
  downloadMediaMessage,
  DisconnectReason,
} from "@whiskeysockets/baileys";
import qrcodeTerminal from "qrcode-terminal";
import QRCode from "qrcode";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { spawn } from "child_process";
import { mkdirSync, writeFileSync, readFileSync, existsSync } from "fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const QR_PNG = "/tmp/wa-login-qr.png";
const logger = pino({ level: "silent" });

const nameFilter = (process.argv[2] || "").toLowerCase();
const numberFilter = (process.env.NUM || "").replace(/\D/g, "");
const CUTOFF = parseInt(process.argv[3] || "0", 10);
const WAIT = parseInt(process.argv[4] || "90", 10) * 1000;
const OUT = resolve(process.argv[5] || "/tmp/erica_wa");
mkdirSync(OUT, { recursive: true });

const contactsByJid = new Map();
const rawByJid = new Map();

function recordContact(c) {
  if (!c?.id) return;
  const name = c.name || c.notify || c.verifiedName || "";
  if (name) {
    const prev = contactsByJid.get(c.id) || "";
    if (name.length > prev.length) contactsByJid.set(c.id, name);
  } else if (!contactsByJid.has(c.id)) contactsByJid.set(c.id, "");
}

function record(m) {
  const jid = m.key?.remoteJid;
  if (!jid || jid === "status@broadcast" || jid.endsWith("@g.us")) return;
  if (m.pushName && !m.key?.fromMe) recordContact({ id: jid, notify: m.pushName });
  if (!rawByJid.has(jid)) rawByJid.set(jid, new Map());
  const id = m.key?.id || `${m.messageTimestamp}-${Math.random()}`;
  rawByJid.get(jid).set(id, m);
}

function describe(m) {
  const msg = m.message || {};
  const doc = msg.documentWithCaptionMessage?.message?.documentMessage;
  let type = "text", text = msg.conversation || msg.extendedTextMessage?.text || "", media = null;
  if (msg.imageMessage) { type = "image"; text = msg.imageMessage.caption || ""; media = { mimetype: msg.imageMessage.mimetype }; }
  else if (msg.videoMessage) { type = "video"; text = msg.videoMessage.caption || ""; media = { mimetype: msg.videoMessage.mimetype }; }
  else if (msg.documentMessage) { type = "document"; text = msg.documentMessage.caption || ""; media = { fileName: msg.documentMessage.fileName, mimetype: msg.documentMessage.mimetype }; }
  else if (doc) { type = "document"; text = doc.caption || ""; media = { fileName: doc.fileName, mimetype: doc.mimetype }; }
  else if (msg.audioMessage) { type = "audio"; media = { mimetype: msg.audioMessage.mimetype, seconds: msg.audioMessage.seconds, ptt: !!msg.audioMessage.ptt }; }
  else if (msg.stickerMessage) { type = "sticker"; media = { mimetype: msg.stickerMessage.mimetype }; }
  return { type, text, media };
}

function extFor(mt, fallback) {
  if (!mt) return fallback || "bin";
  const map = { "image/jpeg": "jpg", "image/png": "png", "image/webp": "webp", "image/gif": "gif",
    "video/mp4": "mp4", "audio/ogg": "ogg", "audio/mpeg": "mp3", "audio/mp4": "m4a",
    "application/pdf": "pdf" };
  return map[mt.split(";")[0]] || (mt.split("/")[1] || fallback || "bin");
}

let viewerOpened = false;
async function showQrWindow(qr) {
  await QRCode.toFile(QR_PNG, qr, { width: 600, margin: 4 });
  if (viewerOpened) return;
  viewerOpened = true;
  const viewer = spawn("feh", ["--auto-zoom", QR_PNG], { detached: true, stdio: "ignore" });
  viewer.on("error", () => spawn("xdg-open", [QR_PNG], { detached: true, stdio: "ignore" }).unref());
  viewer.unref();
  console.error(`  QR-Code-Fenster geöffnet (${QR_PNG}).`);
}

let sock, state, saveCreds;
let collectArmed = false;
let done = false;

async function buildSock() {
  ({ state, saveCreds } = await useMultiFileAuthState(AUTH_DIR));
  const { version } = await fetchLatestBaileysVersion();
  sock = makeWASocket({
    version, logger,
    browser: ["Pegelstand", "CLI", "1.0"],
    auth: { creds: state.creds, keys: makeCacheableSignalKeyStore(state.keys, logger) },
    markOnlineOnConnect: false,
    syncFullHistory: true,
  });
  sock.ev.on("creds.update", saveCreds);
  sock.ev.on("contacts.upsert", (cs) => cs.forEach(recordContact));
  sock.ev.on("contacts.update", (cs) => cs.forEach(recordContact));
  sock.ev.on("chats.upsert", (chats) => chats.forEach((c) => c?.name && recordContact({ id: c.id, name: c.name })));
  sock.ev.on("messaging-history.set", ({ chats, contacts, messages, syncType, progress }) => {
    console.error(`[history.set] chats=${chats?.length||0} contacts=${contacts?.length||0} messages=${messages?.length||0} syncType=${syncType} progress=${progress}`);
    (contacts || []).forEach(recordContact);
    (chats || []).forEach((c) => c?.name && recordContact({ id: c.id, name: c.name }));
    (messages || []).forEach(record);
  });
  sock.ev.on("messages.upsert", ({ messages, type }) => {
    (messages || []).forEach((m) => {
      const jid = m.key?.remoteJid || "";
      if (numberFilter && jid.replace(/\D/g, "").includes(numberFilter))
        console.error(`[upsert type=${type}] TARGET msg @ ${new Date((Number(m.messageTimestamp)||0)*1000).toISOString()} fromMe=${!!m.key?.fromMe}`);
      record(m);
    });
  });

  sock.ev.on("connection.update", (u) => {
    const { connection, qr, lastDisconnect } = u;
    if (qr) {
      console.error("\n  Scan this QR code with WhatsApp (Settings > Linked Devices > Link a Device)\n");
      qrcodeTerminal.generate(qr, { small: true });
      showQrWindow(qr).catch((e) => console.error("  QR-Fenster fehlgeschlagen:", e.message));
    }
    if (connection === "open") {
      console.error("[connected, online] collecting…");
      if (!collectArmed) { collectArmed = true; setTimeout(finish, WAIT); }
    }
    if (connection === "close") {
      if (done) return;
      const code = lastDisconnect?.error?.output?.statusCode;
      console.error(`[close] code=${code}`);
      // 515 = restart required right after pairing — reconnect on the SAME creds.
      if (code === DisconnectReason.restartRequired) {
        console.error("[restart required] reconnecting same session…");
        setTimeout(buildSock, 1000);
      } else if (collectArmed) {
        // Already collected on a live session; salvage whatever we have.
        console.error("[salvaging collected data]");
        finish();
      } else {
        console.error("[closed before collect] giving up");
        process.exit(2);
      }
    }
  });
}

async function finish() {
  if (done) return; done = true;
  const matches = [];
  for (const jid of new Set([...contactsByJid.keys(), ...rawByJid.keys()])) {
    if (jid.endsWith("@g.us")) continue;
    const name = (contactsByJid.get(jid) || "").toLowerCase();
    const terms = nameFilter.split(",").map((t) => t.trim()).filter(Boolean);
    const numHit = numberFilter && jid.replace(/\D/g, "").includes(numberFilter);
    const nameHit = terms.some((t) => name.includes(t));
    if (numHit || nameHit) matches.push({ jid, name: contactsByJid.get(jid) || "" });
  }
  if (matches.length === 0) {
    const dir = [];
    for (const [jid, mm] of rawByJid) dir.push({ jid, name: contactsByJid.get(jid) || "", msgCount: mm.size });
    dir.sort((a, b) => b.msgCount - a.msgCount);
    console.log(JSON.stringify({ me: state?.creds?.me, matches: [], directory: dir.slice(0, 50) }, null, 2));
    try { sock?.end(); } catch {}
    setTimeout(() => process.exit(0), 500); return;
  }
  const STORE = resolve(OUT, "store.json");
  const store = new Map();
  const keyOf = (ts, fromMe) => `${ts}-${fromMe ? 1 : 0}`;
  if (existsSync(STORE)) {
    try { for (const r of JSON.parse(readFileSync(STORE, "utf8"))) store.set(keyOf(r.ts, r.fromMe), r); } catch {}
  }
  const out = { me: state?.creds?.me, cutoff: CUTOFF, outDir: OUT, matches: [] };
  for (const { jid, name } of matches) {
    const all = [...(rawByJid.get(jid) || new Map()).values()]
      .map((m) => ({ raw: m, ts: Number(m.messageTimestamp) || 0 }))
      .filter((x) => x.ts >= CUTOFF)
      .sort((a, b) => a.ts - b.ts);
    for (const { raw, ts } of all) {
      const fromMe = !!raw.key?.fromMe;
      const id = keyOf(ts, fromMe);
      const d = describe(raw);
      const prev = store.get(id) || {};
      const rec = { id, jid, ts, iso: new Date(ts * 1000).toISOString(), fromMe, sender: fromMe ? "me" : (raw.pushName || name), ...d };
      if (!rec.text && prev.text) rec.text = prev.text;
      if (d.media && d.type !== "sticker") {
        const ext = extFor(d.media.mimetype, d.media.fileName?.split(".").pop());
        const fname = `img_${ts}.${ext}`;
        if (prev.file && existsSync(resolve(OUT, prev.file))) { rec.file = prev.file; rec.bytes = prev.bytes; }
        else if (existsSync(resolve(OUT, fname))) { rec.file = fname; }
        else {
          try {
            const buf = await downloadMediaMessage(raw, "buffer", {}, { logger, reuploadRequest: sock.updateMediaMessage });
            writeFileSync(resolve(OUT, fname), buf);
            rec.file = fname; rec.bytes = buf.length;
            console.error(`[downloaded] ${fname} (${buf.length} bytes)`);
          } catch (e) { rec.downloadError = String(e?.message || e); console.error(`[download failed] ${d.type} @ ${rec.iso}: ${rec.downloadError}`); }
        }
      }
      store.set(id, rec);
    }
  }
  const allRecs = [...store.values()].sort((a, b) => a.ts - b.ts);
  writeFileSync(STORE, JSON.stringify(allRecs, null, 2));
  const tjid = matches[0]?.jid, tname = matches[0]?.name || "Erica Baumann";
  out.matches.push({ jid: tjid, name: tname, count: allRecs.length, messages: allRecs });
  writeFileSync(resolve(OUT, "messages.json"), JSON.stringify(out, null, 2));
  console.log(JSON.stringify(out, null, 2));
  try { sock?.end(); } catch {}
  setTimeout(() => process.exit(0), 500);
}

buildSock().catch((e) => { console.error("fatal:", e?.message || e); process.exit(1); });
