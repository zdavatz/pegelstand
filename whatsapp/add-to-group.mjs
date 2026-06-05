#!/usr/bin/env node
// Add one or more participants to a WhatsApp group.
// Usage: node add-to-group.mjs <groupJid> <number> [<number>...]
//   number: bare digits (e.g. 41764374864) or full JID (...@s.whatsapp.net)

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

const [, , groupJid, ...rawNumbers] = process.argv;
if (!groupJid || rawNumbers.length === 0) {
  console.error("Usage: node add-to-group.mjs <groupJid> <number> [<number>...]");
  process.exit(2);
}

const jids = rawNumbers.map((n) =>
  n.includes("@") ? n : `${n.replace(/\D/g, "")}@s.whatsapp.net`
);

const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
const { version } = await fetchLatestBaileysVersion();

const sock = makeWASocket({
  auth: { creds: state.creds, keys: makeCacheableSignalKeyStore(state.keys, logger) },
  version,
  logger,
  browser: ["Pegelstand", "CLI", "1.0"],
  syncFullHistory: false,
  markOnlineOnConnect: false,
});

sock.ev.on("creds.update", saveCreds);

await new Promise((res, rej) => {
  sock.ev.on("connection.update", async ({ connection, lastDisconnect }) => {
    if (connection === "open") {
      try {
        console.log(`Group: ${groupJid}`);
        for (const jid of jids) {
          const onWA = await sock.onWhatsApp(jid);
          if (!onWA || !onWA[0]?.exists) {
            console.log(`  ✗ ${jid} — not on WhatsApp`);
            continue;
          }
          try {
            const result = await sock.groupParticipantsUpdate(groupJid, [jid], "add");
            const status = result?.[0]?.status;
            if (status === "200") console.log(`  ✓ ${jid} — added`);
            else if (status === "403") console.log(`  ! ${jid} — privacy blocks add; invite link needed`);
            else console.log(`  ? ${jid} — status ${status}`);
          } catch (e) {
            console.log(`  ✗ ${jid} — error: ${e.message}`);
          }
        }
      } finally {
        setTimeout(() => { sock.end(); res(); }, 10000);
      }
    }
    if (connection === "close") {
      const err = lastDisconnect?.error;
      if (err?.output?.statusCode === 515) return; // restart required, ignore
    }
  });
});

process.exit(0);
