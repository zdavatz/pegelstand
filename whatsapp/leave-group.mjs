#!/usr/bin/env node
// Leave a WhatsApp group
// Usage: node leave-group.mjs <group-jid>

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
} from "@whiskeysockets/baileys";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const [,, groupJid] = process.argv;

if (!groupJid) {
  console.error("Usage: node leave-group.mjs <group-jid>");
  console.error("Example: node leave-group.mjs 120363401234567890@g.us");
  process.exit(1);
}

const jid = groupJid.includes("@") ? groupJid : `${groupJid}@g.us`;

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
      reject(new Error("Connection timeout (60s)"));
    }, 60000);

    sock.ev.on("connection.update", async (update) => {
      const { connection } = update;

      if (connection === "open") {
        clearTimeout(timeout);
        try {
          console.log(`Leaving group ${jid}...`);
          await sock.groupLeave(jid);
          console.log("Done! Group left.");
          await new Promise((r) => setTimeout(r, 2000));
          sock.end();
          resolve();
        } catch (err) {
          sock.end();
          reject(err);
        }
      }

      if (connection === "close") {
        clearTimeout(timeout);
        // After groupLeave + sock.end(), connection closes — that's expected
        resolve();
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
