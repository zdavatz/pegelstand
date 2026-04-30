#!/usr/bin/env node
// Send a document (any file type) to a WhatsApp contact via Baileys.
// Uses the pegelstand auth/ directory. 5-minute connection timeout and
// 10-second post-send exit delay so the final saveCreds() async write
// actually lands on disk before node exits.
// Usage: node send-doc.mjs <phone-or-jid> <file-path> [caption]

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
  DisconnectReason,
} from "@whiskeysockets/baileys";
import qrcode from "qrcode-terminal";
import pino from "pino";
import { readFileSync, existsSync, rmSync, statSync } from "fs";
import { resolve, dirname, basename, extname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const [,, jidArg, filePath, ...captionParts] = process.argv;
const caption = captionParts.join(" ") || "";

if (!jidArg || !filePath) {
  console.error("Usage: node send-doc.mjs <phone-or-jid> <file-path> [caption]");
  process.exit(1);
}

const absPath = resolve(filePath);
if (!existsSync(absPath)) {
  console.error(`File not found: ${absPath}`);
  process.exit(1);
}

const fileName = basename(absPath);
const ext = extname(absPath).toLowerCase();
const mimeByExt = {
  ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg",
  ".bin": "application/octet-stream", ".csv": "text/csv",
  ".log": "text/plain", ".txt": "text/plain", ".pdf": "application/pdf",
  ".gif": "image/gif", ".mp4": "video/mp4", ".mov": "video/quicktime",
};
const mime = mimeByExt[ext] || "application/octet-stream";
const isImage = ext === ".png" || ext === ".jpg" || ext === ".jpeg";

let jid;
if (jidArg.includes("@")) jid = jidArg;
else if (/^\d+$/.test(jidArg)) jid = `${jidArg}@s.whatsapp.net`;
else { console.error(`Bad JID: ${jidArg}`); process.exit(1); }

console.log(`Target: ${jid}`);
console.log(`File  : ${fileName} (${statSync(absPath).size} bytes, ${mime})`);

let retries = 0;
const MAX_RETRIES = 5;
let done = false;

async function connect() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const { version } = await fetchLatestBaileysVersion();
  console.log(`WA version: ${version.join(".")}`);

  const sock = makeWASocket({
    auth: {
      creds: state.creds,
      keys: makeCacheableSignalKeyStore(state.keys, logger),
    },
    version,
    logger,
    browser: ["Pegelstand", "CLI", "1.0"],
    syncFullHistory: false,
    markOnlineOnConnect: false,
  });

  sock.ev.on("creds.update", saveCreds);

  return new Promise((resolvePromise, reject) => {
    // 5-minute connection timeout — gives plenty of time to scan QR
    const timeout = setTimeout(() => {
      sock.end();
      reject(new Error("Connection timeout (5 min)"));
    }, 300000);

    sock.ev.on("connection.update", async (update) => {
      const { connection, lastDisconnect, qr } = update;

      if (qr) {
        console.log("\n  Scan this QR code with WhatsApp:");
        console.log("  (Settings > Linked Devices > Link a Device)\n");
        qrcode.generate(qr, { small: true });
        console.log("  Waiting for scan (5 min timeout)...\n");
      }

      if (connection === "open") {
        clearTimeout(timeout);
        try {
          console.log(`Connected. Sending to ${jid}...`);
          const buf = readFileSync(absPath);
          const message = isImage
            ? { image: buf, caption, mimetype: mime }
            : { document: buf, fileName, mimetype: mime, caption };

          const sendResult = await Promise.race([
            sock.sendMessage(jid, message),
            new Promise((_, rej) => setTimeout(() => rej(new Error("sendMessage timeout (60s)")), 60000)),
          ]);

          console.log("Sent!", sendResult?.key?.id ? `(id: ${sendResult.key.id})` : "");
          done = true;
          // 10 s exit delay — lets the async saveCreds() writes complete
          // before node tears the process down. Without this, creds.json
          // is often left at 0 bytes and the next send needs a fresh QR.
          console.log("Waiting 10s for creds flush before exit...");
          setTimeout(() => process.exit(0), 10000);
        } catch (err) {
          console.error("Send error:", err.message);
          sock.end();
          reject(err);
        }
      }

      if (connection === "close") {
        clearTimeout(timeout);
        if (done) { resolvePromise(); return; }
        const statusCode = lastDisconnect?.error?.output?.statusCode;
        if (statusCode === DisconnectReason.loggedOut) {
          console.log("Session expired. Clearing auth, QR scan needed.");
          rmSync(AUTH_DIR, { recursive: true, force: true });
          retries = 0;
          connect().then(resolvePromise).catch(reject);
        } else if (retries < MAX_RETRIES) {
          retries++;
          console.log(`Reconnecting (${retries}/${MAX_RETRIES}, close code: ${statusCode ?? "?"})`);
          connect().then(resolvePromise).catch(reject);
        } else {
          reject(new Error(`Failed after ${MAX_RETRIES} retries (status: ${statusCode})`));
        }
      }
    });
  });
}

connect().then(() => process.exit(0)).catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
