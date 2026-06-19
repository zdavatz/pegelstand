#!/usr/bin/env node
// Read-only: connect, go online so the server delivers pending messages for
// this linked device, capture everything from Maik's chat, then on-demand
// fetch older history anchored on the newest message we see. Sends nothing
// except delivery receipts (unavoidable) and a history-sync request.
//
// Usage: node read-maik.mjs <number-digits> [waitSeconds]

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
  DisconnectReason,
} from "@whiskeysockets/baileys";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const numDigits = (process.argv[2] || "").replace(/\D/g, "");
const WAIT = parseInt(process.argv[3] || "75", 10) * 1000;
const TARGET = `${numDigits}@s.whatsapp.net`;

const messages = []; // {ts, fromMe, text, type, hasMedia}
let newest = null;   // {key, ts} for on-demand fetch anchor
let oldest = null;

function describe(m) {
  const msg = m.message || {};
  let type = "text";
  let text =
    msg.conversation ||
    msg.extendedTextMessage?.text ||
    "";
  let hasMedia = false;
  let media = null;
  if (msg.imageMessage) { type = "image"; hasMedia = true; text = msg.imageMessage.caption || ""; media = { mimetype: msg.imageMessage.mimetype, kb: Math.round((msg.imageMessage.fileLength || 0) / 1024) }; }
  else if (msg.videoMessage) { type = "video"; hasMedia = true; text = msg.videoMessage.caption || ""; media = { mimetype: msg.videoMessage.mimetype, kb: Math.round((msg.videoMessage.fileLength || 0) / 1024) }; }
  else if (msg.documentMessage) { type = "document"; hasMedia = true; text = msg.documentMessage.caption || ""; media = { fileName: msg.documentMessage.fileName, mimetype: msg.documentMessage.mimetype, kb: Math.round((msg.documentMessage.fileLength || 0) / 1024) }; }
  else if (msg.documentWithCaptionMessage) { const d = msg.documentWithCaptionMessage.message?.documentMessage || {}; type = "document"; hasMedia = true; text = d.caption || ""; media = { fileName: d.fileName, mimetype: d.mimetype, kb: Math.round((d.fileLength || 0) / 1024) }; }
  else if (msg.audioMessage) { type = "audio"; hasMedia = true; media = { mimetype: msg.audioMessage.mimetype, seconds: msg.audioMessage.seconds }; }
  else if (msg.stickerMessage) { type = "sticker"; hasMedia = true; }
  return { type, text, hasMedia, media };
}

function record(m, source) {
  const jid = m.key?.remoteJid;
  if (jid !== TARGET) return;
  const ts = Number(m.messageTimestamp) || 0;
  const d = describe(m);
  messages.push({
    ts,
    iso: ts ? new Date(ts * 1000).toISOString() : "",
    fromMe: !!m.key?.fromMe,
    source,
    ...d,
  });
  if (!newest || ts > newest.ts) newest = { key: m.key, ts };
  if (!oldest || ts < oldest.ts) oldest = { key: m.key, ts };
  console.error(`[${source}] ${d.type}${d.hasMedia ? "*" : ""} @ ${ts ? new Date(ts * 1000).toISOString() : "?"} fromMe=${!!m.key?.fromMe}`);
}

async function main() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const { version } = await fetchLatestBaileysVersion();

  const sock = makeWASocket({
    version,
    logger,
    auth: { creds: state.creds, keys: makeCacheableSignalKeyStore(state.keys, logger) },
    markOnlineOnConnect: true,
    syncFullHistory: true,
  });

  sock.ev.on("creds.update", saveCreds);

  sock.ev.on("messaging-history.set", ({ messages: ms, progress, syncType }) => {
    console.error(`[history.set] messages=${ms?.length || 0} progress=${progress} syncType=${syncType}`);
    (ms || []).forEach((m) => record(m, "history"));
  });

  sock.ev.on("messages.upsert", ({ messages: ms, type }) => {
    console.error(`[upsert type=${type}] n=${ms?.length || 0}`);
    (ms || []).forEach((m) => record(m, "upsert"));
  });

  let askedHistory = false;
  sock.ev.on("connection.update", async (u) => {
    const { connection, lastDisconnect } = u;
    if (connection === "open") {
      console.error("[connected, online] collecting…");
      setTimeout(async () => {
        // on-demand: pull older history from Maik's chat if we have an anchor
        if (!askedHistory && oldest?.key) {
          askedHistory = true;
          try {
            console.error("[fetchMessageHistory] requesting 50 older…");
            await sock.fetchMessageHistory(50, oldest.key, oldest.ts);
          } catch (e) {
            console.error("[fetchMessageHistory failed]", e?.message || e);
          }
        }
      }, 8000);
      setTimeout(finish, WAIT);
    }
    if (connection === "close") {
      const code = lastDisconnect?.error?.output?.statusCode;
      console.error(`[close] code=${code}`);
      if (code === DisconnectReason.loggedOut) process.exit(2);
    }
  });

  let done = false;
  function finish() {
    if (done) return;
    done = true;
    messages.sort((a, b) => a.ts - b.ts);
    // dedupe by id
    const seen = new Set();
    const uniq = messages.filter((m) => {
      const k = `${m.ts}|${m.text}|${m.type}`;
      if (seen.has(k)) return false;
      seen.add(k);
      return true;
    });
    console.log(JSON.stringify({ target: TARGET, count: uniq.length, messages: uniq }, null, 2));
    sock.end();
    setTimeout(() => process.exit(0), 500);
  }
}

main().catch((e) => { console.error("fatal:", e?.message || e); process.exit(1); });
