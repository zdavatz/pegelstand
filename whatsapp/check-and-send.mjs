#!/usr/bin/env node
// Verify which phone numbers are registered on WhatsApp and send each one a
// welcome message (with optional image attachment). Driven by pegelstand's
// `sync-contacts` command.
//
// Usage: node check-and-send.mjs <job.json> <out.json>
//   job.json  = {
//                 contacts:  [{number, jid, firstName, lastName}],
//                 welcome?:  "Hallo {first}!",   // caption / message body
//                 imagePath?: "/abs/path/foo.png", // if set, sent as image+caption
//                 groupJid?: "...@g.us"            // if set, add registered contacts to this group
//               }
//   out.json  = [{number, jid, registered, sent, error?}]
//
// Welcome message supports {first}, {last}, {name} placeholders.

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
  DisconnectReason,
} from "@whiskeysockets/baileys";
import qrcode from "qrcode-terminal";
import pino from "pino";
import { readFileSync, writeFileSync, rmSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const [,, jobPath, outPath] = process.argv;
if (!jobPath || !outPath) {
  console.error("Usage: node check-and-send.mjs <job.json> <out.json>");
  process.exit(1);
}

const job = JSON.parse(readFileSync(jobPath, "utf8"));
const { contacts, welcome, imagePath, groupJid } = job;
if (!Array.isArray(contacts) || contacts.length === 0) {
  writeFileSync(outPath, "[]");
  process.exit(0);
}

const imageBuf = imagePath ? readFileSync(imagePath) : null;
const imageMime = imagePath && imagePath.toLowerCase().endsWith(".jpg") ? "image/jpeg" : "image/png";

console.log(
  `${contacts.length} contact(s) to check` +
  (imageBuf ? " + send image" : welcome ? " + send welcome text" : " (check only)")
);

let retries = 0;
const MAX_RETRIES = 5;
let done = false;

function personalize(template, c) {
  return template
    .replace(/\{first\}/g, c.firstName || "")
    .replace(/\{last\}/g,  c.lastName  || "")
    .replace(/\{date\}/g,  c.date      || "")
    .replace(/\{name\}/g,  `${c.firstName || ""} ${c.lastName || ""}`.trim());
}

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
    // 5-minute connection timeout — allows time to scan QR if needed.
    const timeout = setTimeout(() => {
      sock.end();
      reject(new Error("Connection timeout (5 min)"));
    }, 300000);

    sock.ev.on("connection.update", async (update) => {
      const { connection, lastDisconnect, qr } = update;

      if (qr) {
        console.log("\n  Scan QR with WhatsApp → Linked Devices → Link a Device\n");
        qrcode.generate(qr, { small: true });
      }

      if (connection === "open") {
        clearTimeout(timeout);
        const results = [];
        // Fetched lazily on the first privacy-blocked add, then reused.
        let groupInviteLink = null;
        try {
          for (const c of contacts) {
            const result = { number: c.number, jid: c.jid, registered: false, sent: false };
            try {
              const lookup = await sock.onWhatsApp(c.jid);
              const hit = Array.isArray(lookup) ? lookup[0] : null;
              if (hit && hit.exists) {
                result.registered = true;
                if (hit.jid) result.jid = hit.jid;
                const caption = welcome ? personalize(welcome, c) : "";
                if (imageBuf) {
                  await sock.sendMessage(result.jid, {
                    image: imageBuf, caption, mimetype: imageMime,
                  });
                  result.sent = true;
                  console.log(`  ✓ ${c.number} (${c.firstName || ""}) — image + caption sent`);
                  await new Promise((r) => setTimeout(r, 1500)); // gentle rate-limit
                } else if (welcome) {
                  await sock.sendMessage(result.jid, { text: caption });
                  result.sent = true;
                  console.log(`  ✓ ${c.number} (${c.firstName || ""}) — text sent`);
                  await new Promise((r) => setTimeout(r, 1500));
                } else {
                  console.log(`  ✓ ${c.number} (${c.firstName || ""}) — registered`);
                }
                if (groupJid) {
                  try {
                    const r = await sock.groupParticipantsUpdate(groupJid, [result.jid], "add");
                    const st = r?.[0]?.status;
                    if (st === "200")      console.log(`    → added to group`);
                    else if (st === "409") console.log(`    → already in group`);
                    else if (st === "403") {
                      // Privacy settings block a direct add — send the invite link instead.
                      try {
                        if (!groupInviteLink) {
                          const code = await sock.groupInviteCode(groupJid);
                          groupInviteLink = `https://chat.whatsapp.com/${code}`;
                        }
                        const first = c.firstName || "";
                        const inviteMsg =
                          (first ? `Hallo ${first}! ` : "") +
                          "Ich konnte dich wegen deiner Datenschutz-Einstellungen nicht " +
                          "direkt zur Gruppe hinzufügen. Hier ist der Einladungslink: " +
                          groupInviteLink;
                        await sock.sendMessage(result.jid, { text: inviteMsg });
                        result.invited = true;
                        console.log(`    → privacy blocks add; invite link sent`);
                        await new Promise((r) => setTimeout(r, 1500));
                      } catch (e) {
                        console.log(`    → privacy blocks add; invite link FAILED: ${e.message}`);
                      }
                    }
                    else                   console.log(`    → group add status ${st}`);
                  } catch (e) {
                    console.log(`    → group add error: ${e.message}`);
                  }
                  await new Promise((r) => setTimeout(r, 1000));
                }
              } else {
                console.log(`  ✗ ${c.number} (${c.firstName || ""}) — not on WhatsApp`);
              }
            } catch (err) {
              result.error = err.message;
              console.log(`  ! ${c.number} — ${err.message}`);
            }
            results.push(result);
          }
          writeFileSync(outPath, JSON.stringify(results, null, 2));
          done = true;
          console.log(`Done. Waiting 10s for creds flush before exit...`);
          setTimeout(() => process.exit(0), 10000);
        } catch (err) {
          console.error("Fatal:", err.message);
          writeFileSync(outPath, JSON.stringify(results, null, 2));
          sock.end();
          reject(err);
        }
      }

      if (connection === "close") {
        clearTimeout(timeout);
        if (done) { resolvePromise(); return; }
        const code = lastDisconnect?.error?.output?.statusCode;
        if (code === DisconnectReason.loggedOut) {
          console.log("Session expired. Clearing auth, QR scan needed.");
          rmSync(AUTH_DIR, { recursive: true, force: true });
          retries = 0;
          connect().then(resolvePromise).catch(reject);
        } else if (retries < MAX_RETRIES) {
          retries++;
          console.log(`Reconnecting (${retries}/${MAX_RETRIES}, code: ${code ?? "?"})`);
          connect().then(resolvePromise).catch(reject);
        } else {
          reject(new Error(`Failed after ${MAX_RETRIES} retries (status: ${code})`));
        }
      }
    });
  });
}

connect().then(() => process.exit(0)).catch((err) => {
  console.error("Error:", err.message);
  process.exit(1);
});
