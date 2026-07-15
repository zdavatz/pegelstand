#!/usr/bin/env node
// Diagnostic: resolve a number on WhatsApp, send the pumper welcome once, and
// watch the delivery receipt (server-ack vs delivery-ack vs read) for ~50s.
// Usage: node diag-delivery.mjs <number> <first> <date> <imagePath>
import makeWASocket, {
  useMultiFileAuthState, makeCacheableSignalKeyStore, fetchLatestBaileysVersion,
} from "@whiskeysockets/baileys";
import pino from "pino";
import { readFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const [,, number, first, date, imagePath] = process.argv;
const jidGuess = number.replace(/^\+/, "") + "@s.whatsapp.net";
const STATUS = { 0: "ERROR", 1: "PENDING", 2: "SERVER_ACK", 3: "DELIVERY_ACK", 4: "READ", 5: "PLAYED" };

const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
const { version } = await fetchLatestBaileysVersion();
const sock = makeWASocket({
  auth: { creds: state.creds, keys: makeCacheableSignalKeyStore(state.keys, logger) },
  version, logger, browser: ["Pegelstand", "CLI", "1.0"],
  syncFullHistory: false, markOnlineOnConnect: false,
});
sock.ev.on("creds.update", saveCreds);

let sentKeyId = null;
let maxStatus = 0;

sock.ev.on("messages.update", (updates) => {
  for (const u of updates) {
    if (sentKeyId && u.key?.id === sentKeyId && typeof u.update?.status === "number") {
      maxStatus = Math.max(maxStatus, u.update.status);
      console.log(`  receipt: ${STATUS[u.update.status] || u.update.status}`);
    }
  }
});

sock.ev.on("connection.update", async (upd) => {
  if (upd.qr) { console.log("QR needed — session not authed"); process.exit(2); }
  if (upd.connection !== "open") return;
  try {
    const lookup = await sock.onWhatsApp(jidGuess);
    console.log("onWhatsApp:", JSON.stringify(lookup));
    const hit = Array.isArray(lookup) ? lookup[0] : null;
    if (!hit || !hit.exists) { console.log("NOT REGISTERED on WhatsApp"); process.exit(0); }
    const target = hit.jid || jidGuess;
    console.log("resolved target jid:", target);

    const caption = `Hallo ${first}! Willkommen bei Pump Tsüri! Deine Lektion ist am ${date}. ` +
      `Wir beginnen um 7 Uhr in der früh! Ort: https://maps.app.goo.gl/gQRDeSW8Jtpce1CY9 — ` +
      `Anbei die Wassertemperatur vom Zürichsee der letzten 3 Tage.`;
    const img = readFileSync(imagePath);
    const sent = await sock.sendMessage(target, { image: img, caption, mimetype: "image/png" });
    sentKeyId = sent?.key?.id;
    console.log("sent, key.id:", sentKeyId);
    console.log("watching receipts for 50s...");
    setTimeout(() => {
      console.log(`FINAL max delivery status: ${STATUS[maxStatus] || maxStatus} (${maxStatus})`);
      console.log(maxStatus >= 3 ? "=> DELIVERED to device" : "=> NOT confirmed delivered (stuck at server / restricted)");
      process.exit(0);
    }, 50000);
  } catch (e) {
    console.log("ERR:", e.message);
    process.exit(1);
  }
});
