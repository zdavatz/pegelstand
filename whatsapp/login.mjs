#!/usr/bin/env node
// WhatsApp login — scan QR code, save session
// Usage: node login.mjs

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
} from "@whiskeysockets/baileys";
import qrcode from "qrcode-terminal";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

async function main() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const { version } = await fetchLatestBaileysVersion();

  console.log(`WhatsApp version: ${version.join(".")}`);

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

  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      sock.end();
      reject(new Error("Timeout (90s) — kein QR-Code gescannt"));
    }, 90000);

    sock.ev.on("connection.update", (update) => {
      const { connection, qr } = update;

      if (qr) {
        console.log("\n  Scan this QR code with WhatsApp:");
        console.log("  (Settings > Linked Devices > Link a Device)\n");
        qrcode.generate(qr, { small: true });
      }

      if (connection === "open") {
        clearTimeout(timeout);
        console.log("\n  Login erfolgreich! Session gespeichert.");
        console.log(`  Auth: ${AUTH_DIR}\n`);
        sock.end();
        resolve();
      }

      if (connection === "close") {
        clearTimeout(timeout);
        reject(new Error("Verbindung geschlossen"));
      }
    });
  });
}

main()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error("Error:", err.message);
    process.exit(1);
  });
