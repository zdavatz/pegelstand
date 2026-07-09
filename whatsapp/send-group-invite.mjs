#!/usr/bin/env node
// Send a WhatsApp group invite link to one or more people.
// Use when someone's privacy settings block a direct group add (status 403).
// Usage: node send-group-invite.mjs <groupJid> <number> [<number>...]
//   number: bare digits (e.g. 41765490823) or full JID (...@s.whatsapp.net)

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
  console.error("Usage: node send-group-invite.mjs <groupJid> <number> [<number>...]");
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

await new Promise((res) => {
  sock.ev.on("connection.update", async ({ connection, lastDisconnect }) => {
    if (connection === "open") {
      try {
        const code = await sock.groupInviteCode(groupJid);
        const link = `https://chat.whatsapp.com/${code}`;
        console.log(`Group: ${groupJid}`);
        console.log(`Invite link: ${link}`);
        for (const jid of jids) {
          const onWA = await sock.onWhatsApp(jid);
          if (!onWA || !onWA[0]?.exists) {
            console.log(`  ✗ ${jid} — not on WhatsApp`);
            continue;
          }
          try {
            const msg =
              "Ich konnte dich wegen deiner Datenschutz-Einstellungen nicht " +
              "direkt zur Gruppe hinzufügen. Hier ist der Einladungslink: " +
              link;
            await sock.sendMessage(onWA[0].jid, { text: msg });
            console.log(`  ✓ ${jid} — invite link sent`);
            await new Promise((r) => setTimeout(r, 1500));
          } catch (e) {
            console.log(`  ✗ ${jid} — error: ${e.message}`);
          }
        }
      } catch (e) {
        console.log(`  ! error: ${e.message}`);
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
