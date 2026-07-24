#!/usr/bin/env node
// Send a text message with @-mentions to a WhatsApp group via Baileys.
// Usage: node send-group-mention.mjs <job.json>
//
// Job JSON: { "groupJid": "…@g.us",
//             "text": "für morgen angemeldet sind: @4179… @4178…",
//             "mentions": ["4179…@s.whatsapp.net", "4178…@s.whatsapp.net"] }
//
// The text must already contain the "@<number>" tokens — WhatsApp renders each
// one as the mentioned person's display name. `mentions` carries the JIDs that
// Baileys puts into contextInfo.mentionedJid.

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

const [, , jobPath] = process.argv;
if (!jobPath) {
  console.error("Usage: node send-group-mention.mjs <job.json>");
  process.exit(1);
}
if (!existsSync(jobPath)) {
  console.error(`Job file not found: ${jobPath}`);
  process.exit(1);
}

const job = JSON.parse(readFileSync(jobPath, "utf8"));
const groupJid = job.groupJid;
const text = job.text;
const mentions = Array.isArray(job.mentions) ? job.mentions : [];

if (!groupJid || !groupJid.includes("@g.us")) {
  console.error("job.groupJid missing or not a group JID (…@g.us)");
  process.exit(1);
}
if (!text || !text.trim()) {
  console.error("job.text missing");
  process.exit(1);
}

// Same 10s post-send grace as send-doc.mjs: saveCreds() is async, and exiting
// before it lands leaves auth/creds.json stale (next send would need a new QR).
const POST_SEND_MS = Number(process.env.WA_KEEPALIVE_MS || 10000);

// Mentioning someone who is not in the group posts their raw phone number to
// every member — a leak you cannot take back. Refuse by default; the caller can
// opt in once it has seen who is affected.
const ALLOW_NONMEMBERS = process.env.WA_ALLOW_NONMEMBER_MENTIONS === "1";

let retries = 0;
const MAX_RETRIES = 3;
let done = false;

async function connect() {
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
          // A mention only renders as a *name* if the mentioned person is in the
          // group. For non-members WhatsApp shows the raw phone number to
          // everyone, so stop before posting unless explicitly allowed.
          try {
            const meta = await sock.groupMetadata(groupJid);
            const members = new Set(
              (meta.participants || []).map((p) => (p.id || "").split(":")[0].split("@")[0]),
            );
            const outsiders = mentions.filter(
              (j) => !members.has(j.split("@")[0]),
            );
            console.log(`Gruppe: ${meta.subject} (${meta.participants?.length ?? "?"} Mitglieder)`);
            if (outsiders.length) {
              console.log(
                `  ⚠ ${outsiders.length} Erwähnte sind NICHT in der Gruppe — bei ihnen ` +
                  `würde WhatsApp allen die Telefonnummer statt des Namens zeigen:`,
              );
              for (const o of outsiders) console.log(`      +${o.split("@")[0]}`);
              if (!ALLOW_NONMEMBERS) {
                console.error(
                  "\n  ABBRUCH: nichts gesendet — sonst stehen diese Nummern für alle\n" +
                    "  sichtbar in der Gruppe. Entweder die Leute zuerst in die Gruppe\n" +
                    "  aufnehmen, oder bewusst trotzdem posten:\n" +
                    "    pegelstand welcome --announce … --announce-allow-nonmembers",
                );
                process.exit(2);
              }
              console.log("  → WA_ALLOW_NONMEMBER_MENTIONS=1 gesetzt, sende trotzdem.");
            }
          } catch (e) {
            console.log(`  (Gruppen-Metadaten nicht lesbar: ${e.message})`);
          }

          console.log(`Sende an ${groupJid} (${mentions.length} Erwähnungen)...`);
          const res = await Promise.race([
            sock.sendMessage(groupJid, { text, mentions }),
            new Promise((_, rej) =>
              setTimeout(() => rej(new Error("sendMessage timeout (30s)")), 30000),
            ),
          ]);

          console.log("Gesendet!", res?.key?.id ? `(id: ${res.key.id})` : "");
          done = true;
          setTimeout(() => process.exit(0), POST_SEND_MS);
        } catch (err) {
          console.error("Send error:", err.message);
          sock.end();
          reject(err);
        }
      }

      if (connection === "close") {
        clearTimeout(timeout);
        if (done) {
          resolvePromise();
          return;
        }
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
