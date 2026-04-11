#!/usr/bin/env node
// Send an image to a WhatsApp group via Baileys
// Usage: node send.mjs <group-jid> <image-path> [caption]
// First run: scan QR code with WhatsApp → session saved in auth/

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
  DisconnectReason,
} from "@whiskeysockets/baileys";
import qrcode from "qrcode-terminal";
import pino from "pino";
import { readFileSync, existsSync, rmSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const [,, groupJid, imagePath, ...captionParts] = process.argv;
const caption = captionParts.join(" ") || "";

if (!groupJid || !imagePath) {
  console.error("Usage: node send.mjs <group-jid> <image-path> [caption]");
  console.error("Example: node send.mjs 120363401234567890@g.us ./png/zurichsee.png 'Zürichsee Report'");
  process.exit(1);
}

const absPath = resolve(imagePath);
if (!existsSync(absPath)) {
  console.error(`File not found: ${absPath}`);
  process.exit(1);
}

const jid = groupJid.includes("@") ? groupJid : `${groupJid}@g.us`;

let retries = 0;
const MAX_RETRIES = 3;

async function connect() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const { version } = await fetchLatestBaileysVersion();

  console.log(`Using WA version: ${version.join(".")}`);

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
    const timeout = setTimeout(() => {
      sock.end();
      reject(new Error("Connection timeout (90s)"));
    }, 90000);

    sock.ev.on("connection.update", async (update) => {
      const { connection, lastDisconnect, qr } = update;

      if (qr) {
        console.log("\n  Scan this QR code with WhatsApp:");
        console.log("  (Settings > Linked Devices > Link a Device)\n");
        qrcode.generate(qr, { small: true });
        console.log("  Waiting for scan...\n");
      }

      if (connection === "open") {
        clearTimeout(timeout);
        try {
          console.log(`Connected! Sending image to ${jid}...`);

          const imageBuffer = readFileSync(absPath);
          const mime = absPath.endsWith(".png") ? "image/png" : "image/jpeg";

          await sock.sendMessage(jid, {
            image: imageBuffer,
            caption: caption,
            mimetype: mime,
          });

          console.log("Sent!");
          await new Promise((r) => setTimeout(r, 2000));
          sock.end();
          resolvePromise();
        } catch (err) {
          sock.end();
          reject(err);
        }
      }

      if (connection === "close") {
        clearTimeout(timeout);
        const statusCode = lastDisconnect?.error?.output?.statusCode;

        if (statusCode === DisconnectReason.loggedOut) {
          console.log("Session expired. Clearing auth, please scan again...");
          rmSync(AUTH_DIR, { recursive: true, force: true });
          retries = 0;
          connect().then(resolvePromise).catch(reject);
        } else if (retries < MAX_RETRIES) {
          retries++;
          connect().then(resolvePromise).catch(reject);
        } else {
          reject(new Error(`Connection failed after ${MAX_RETRIES} retries (status: ${statusCode})`));
        }
      }
    });
  });
}

connect()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error("Error:", err.message);
    process.exit(1);
  });
