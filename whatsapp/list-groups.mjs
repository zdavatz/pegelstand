#!/usr/bin/env node
// List all WhatsApp groups with their JIDs
// Usage: node list-groups.mjs
// Requires: session already set up (run send.mjs first to scan QR)

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
      reject(new Error("Connection timeout (90s)"));
    }, 90000);

    sock.ev.on("connection.update", async (update) => {
      const { connection, qr } = update;

      if (qr) {
        console.log("\n  Scan this QR code with WhatsApp:");
        console.log("  (Settings > Linked Devices > Link a Device)\n");
        qrcode.generate(qr, { small: true });
      }

      if (connection === "open") {
        clearTimeout(timeout);

        console.log("Connected. Fetching groups...\n");

        const groups = await sock.groupFetchAllParticipating();
        const sorted = Object.values(groups).sort((a, b) => a.subject.localeCompare(b.subject));

        console.log(`Found ${sorted.length} groups:\n`);
        for (const g of sorted) {
          console.log(`  ${g.id}  ${g.subject}`);
        }

        await new Promise((r) => setTimeout(r, 1000));
        sock.end();
        resolve();
      }

      if (connection === "close") {
        clearTimeout(timeout);
        reject(new Error("Connection closed"));
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
