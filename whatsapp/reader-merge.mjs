#!/usr/bin/env node
// Read-only: connect & go online, collect messages from chats matching a name
// filter, download attachments for messages since a cutoff timestamp, write
// media files + a metadata JSON. Sends nothing but unavoidable receipts.
//
// Usage: node reader-merge.mjs <nameFilter> <cutoffUnixSeconds> [waitSeconds] [outDir]

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
  downloadMediaMessage,
  DisconnectReason,
} from "@whiskeysockets/baileys";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { mkdirSync, writeFileSync, readFileSync, existsSync } from "fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const nameFilter = (process.argv[2] || "").toLowerCase();
const numberFilter = (process.env.NUM || "").replace(/\D/g, "");
const CUTOFF = parseInt(process.argv[3] || "0", 10);
const WAIT = parseInt(process.argv[4] || "75", 10) * 1000;
const OUT = resolve(process.argv[5] || "/tmp/erica_wa");
mkdirSync(OUT, { recursive: true });

const contactsByJid = new Map();      // jid -> best display name
const rawByJid = new Map();           // jid -> Map(msgId -> raw message)

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

async function main() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const { version } = await fetchLatestBaileysVersion();
  const sock = makeWASocket({
    version, logger,
    auth: { creds: state.creds, keys: makeCacheableSignalKeyStore(state.keys, logger) },
    markOnlineOnConnect: true,
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
    const { connection, lastDisconnect } = u;
    if (connection === "open") { console.error("[connected, online] collecting…"); setTimeout(finish, WAIT); }
    if (connection === "close") {
      const code = lastDisconnect?.error?.output?.statusCode;
      console.error(`[close] code=${code}`);
      if (code === DisconnectReason.loggedOut) process.exit(2);
    }
  });

  let done = false;
  async function finish() {
    if (done) return; done = true;
    // resolve matching jids
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
      // no name match -> dump directory to help identify
      const dir = [];
      for (const [jid, mm] of rawByJid) dir.push({ jid, name: contactsByJid.get(jid) || "", msgCount: mm.size });
      dir.sort((a, b) => b.msgCount - a.msgCount);
      console.log(JSON.stringify({ me: state.creds?.me, matches: [], directory: dir.slice(0, 50) }, null, 2));
      sock.end(); setTimeout(() => process.exit(0), 500); return;
    }
    // Load persistent merge store (keyed by message id) so repeated syncs accumulate.
    const STORE = resolve(OUT, "store.json");
    const store = new Map();
    const keyOf = (ts, fromMe) => `${ts}-${fromMe ? 1 : 0}`;
    if (existsSync(STORE)) {
      try { for (const r of JSON.parse(readFileSync(STORE, "utf8"))) store.set(keyOf(r.ts, r.fromMe), r); } catch {}
    }

    const out = { me: state.creds?.me, cutoff: CUTOFF, outDir: OUT, matches: [] };
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
        // keep a previously captioned text if this delivery lacks it
        if (!rec.text && prev.text) rec.text = prev.text;
        if (d.media && d.type !== "sticker") {
          const ext = extFor(d.media.mimetype, d.media.fileName?.split(".").pop());
          const fname = `img_${ts}.${ext}`;
          if (prev.file && existsSync(resolve(OUT, prev.file))) {
            rec.file = prev.file; rec.bytes = prev.bytes;
          } else if (existsSync(resolve(OUT, fname))) {
            rec.file = fname;
          } else {
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
    void 0;
    // persist store and emit merged messages.json grouped under the target jid
    const allRecs = [...store.values()].sort((a, b) => a.ts - b.ts);
    writeFileSync(STORE, JSON.stringify(allRecs, null, 2));
    const tjid = matches[0]?.jid, tname = matches[0]?.name || "Erica Baumann";
    out.matches.push({ jid: tjid, name: tname, count: allRecs.length, messages: allRecs });
    writeFileSync(resolve(OUT, "messages.json"), JSON.stringify(out, null, 2));
    console.log(JSON.stringify(out, null, 2));
    sock.end(); setTimeout(() => process.exit(0), 500);
  }
}
main().catch((e) => { console.error("fatal:", e?.message || e); process.exit(1); });
