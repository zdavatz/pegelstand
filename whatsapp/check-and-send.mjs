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

// Sent-message store so Baileys can answer decryption retry-receipts: when a
// recipient can't decrypt (common right after re-linking → "Waiting for this
// message"), their device asks the sender to re-encrypt. Baileys does that
// automatically IF the socket is still alive AND getMessage returns the original
// content. Set WA_KEEPALIVE_MS to stay connected long enough to serve retries.
const sentStore = new Map();

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

// --watch-delivery (WA_WATCH_DELIVERY=1): after sending, keep the socket open
// and watch messages.update receipts so we can report the REAL delivery ack per
// contact (SERVER_ACK=2 accepted-by-server vs DELIVERY_ACK=3 reached-device vs
// READ=4) — no duplicate send needed. Default keepalive stretches to 50s so the
// device has time to ack; the highest ack seen is written back into out.json.
// NOTE: the ack listener now runs ALWAYS, not just under --watch-delivery.
// Reason: WhatsApp silently drops first-contact messages to people who have
// never messaged us (see whatsapp/CLAUDE.md). sendMessage() still returns a
// key, so a dropped send used to be reported as `sent: true` and the contact
// got marked as greeted — they were then never retried. A real send gets
// SERVER_ACK within ~1s; a dropped one gets nothing at all. So we require at
// least SERVER_ACK before reporting `sent`. --watch-delivery only widens the
// window (50s) and adds per-receipt logging for DELIVERY_ACK / READ.
const watchDelivery = process.env.WA_WATCH_DELIVERY === "1";
const ACK = { 0: "NO_RECEIPT", 1: "PENDING", 2: "SERVER_ACK", 3: "DELIVERY_ACK", 4: "READ", 5: "PLAYED" };
const trackedKeys = new Map(); // sent message key.id -> its result object (same ref as in `results`)
const deliveryMax = new Map(); // sent message key.id -> highest ack status int seen so far

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
    getMessage: async (key) => sentStore.get(key.id) || undefined,
  });

  sock.ev.on("creds.update", saveCreds);

  sock.ev.on("messages.update", (updates) => {
    for (const u of updates) {
      const id = u.key?.id;
      if (!id || !trackedKeys.has(id) || typeof u.update?.status !== "number") continue;
      const prev = deliveryMax.get(id) || 0;
      if (u.update.status > prev) {
        deliveryMax.set(id, u.update.status);
        const r = trackedKeys.get(id);
        if (watchDelivery) {
          console.log(`    ↳ ${r.number} receipt: ${ACK[u.update.status] || u.update.status}`);
        }
      }
    }
  });

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
                  const sent = await sock.sendMessage(result.jid, {
                    image: imageBuf, caption, mimetype: imageMime,
                  });
                  if (sent?.key?.id) {
                    sentStore.set(sent.key.id, sent.message);
                    trackedKeys.set(sent.key.id, result);
                  }
                  // `sent` stays false until an ack proves it left the server.
                  result.handedOff = true;
                  console.log(`  → ${c.number} (${c.firstName || ""}) — image + caption handed off, awaiting ack`);
                  await new Promise((r) => setTimeout(r, 1500)); // gentle rate-limit
                } else if (welcome) {
                  const sent = await sock.sendMessage(result.jid, { text: caption });
                  if (sent?.key?.id) {
                    sentStore.set(sent.key.id, sent.message);
                    trackedKeys.set(sent.key.id, result);
                  }
                  result.handedOff = true;
                  console.log(`  → ${c.number} (${c.firstName || ""}) — text handed off, awaiting ack`);
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
          const keepaliveMs = parseInt(
            process.env.WA_KEEPALIVE_MS || (watchDelivery ? "50000" : "10000"), 10);
          console.log(
            `Done. Staying connected ${Math.round(keepaliveMs / 1000)}s ` +
            (watchDelivery
              ? "(watching delivery receipts + creds flush) before exit..."
              : "(retry-receipt handling + creds flush) before exit...")
          );
          setTimeout(() => {
            // Attach the highest ack seen and decide `sent` from it. Runs
            // unconditionally: a message with no ack at all never left the
            // server (WhatsApp drops first contacts silently), so reporting it
            // as sent would mark the person greeted and they'd never be
            // retried. SERVER_ACK (2) is the minimum proof of transmission.
            for (const [id, r] of trackedKeys) {
              const s = deliveryMax.get(id) || 0;
              r.delivery = ACK[s] || String(s);
              r.deliveryStatus = s;
              r.sent = s >= 2;
              if (!r.sent) {
                r.error = "no ack — message dropped (recipient likely never messaged us first)";
              }
            }
            writeFileSync(outPath, JSON.stringify(results, null, 2));
            const handed = results.filter((r) => r.handedOff);
            if (handed.length) {
              console.log("Delivery summary:");
              for (const r of handed) {
                const s = r.deliveryStatus || 0;
                const mark = s >= 3 ? "✓" : s >= 2 ? "○" : "✗";
                const note = s >= 3 ? "DELIVERED to device"
                           : s >= 2 ? "accepted by server, no device ack yet"
                           : "DROPPED — no receipt at all (recipient never messaged us first?)";
                console.log(`  ${mark} ${r.number} — ${ACK[s] || s} (${s}) ${note}`);
              }
              const dropped = handed.filter((r) => !r.sent).length;
              if (dropped) {
                console.log(
                  `  ${dropped} message(s) dropped — these contacts are NOT marked as greeted ` +
                  `and will be retried / mailed on the next run.`
                );
              }
            }
            process.exit(0);
          }, keepaliveMs);
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
