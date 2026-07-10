#!/usr/bin/env node
// WhatsApp login via PAIRING CODE (no QR — meant for remote/headless).
// Usage: node login-pair.mjs <phone-number> [--force]
//   phone-number: international format, digits only, no '+', e.g. 41791234567
// In WhatsApp: Settings → Linked Devices → Link a Device →
//              "Link with phone number instead" → enter the printed code.
//
// ⚠️ EXPERIMENTAL / UNRELIABLE — prefer login-qr.mjs.
// On 2026-07-10 this failed 4× in a row against Baileys 7.0.0-rc13: the phone
// showed "Gerät konnte nicht hinzugefügt werden", the socket stayed stuck at
// registered:false, and it closed with code 408 ("attempts ended"). QR login
// (login-qr.mjs) worked on the first scan immediately after — so it was the
// RC pairing-code path, not a WhatsApp block. Use QR to re-login; keep this
// only in case a stable Baileys release fixes the pairing-code handshake.

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
} from "@whiskeysockets/baileys";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { rmSync, existsSync, readdirSync } from "fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const args = process.argv.slice(2);
const force = args.includes("--force");
const phone = (args.find((a) => !a.startsWith("--")) || "").replace(/\D/g, "");

if (!phone) {
  console.error("Usage: node login-pair.mjs <phone-number> [--force]");
  console.error("  phone-number: international, digits only, e.g. 41791234567");
  process.exit(2);
}

if (force && existsSync(AUTH_DIR)) {
  console.log("Lösche alte Session (--force)...");
  rmSync(AUTH_DIR, { recursive: true, force: true });
}

async function startSocket() {
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
    printQRInTerminal: false,
    browser: ["Pegelstand", "CLI", "1.0"],
    syncFullHistory: false,
    markOnlineOnConnect: false,
  });

  sock.ev.on("creds.update", saveCreds);
  return sock;
}

async function loginOnce(requestCode) {
  const sock = await startSocket();

  // Only request a pairing code if this session is not yet registered.
  if (requestCode && !sock.authState.creds.registered) {
    // A short delay after socket creation makes the request reliable.
    await new Promise((r) => setTimeout(r, 3000));
    try {
      const code = await sock.requestPairingCode(phone);
      const pretty = code?.length === 8 ? `${code.slice(0, 4)}-${code.slice(4)}` : code;
      console.log("\n  ====================================");
      console.log(`   PAIRING-CODE:  ${pretty}`);
      console.log("  ====================================");
      console.log(`  Nummer: +${phone}`);
      console.log("  In WhatsApp: Verknüpfte Geräte → Gerät verknüpfen →");
      console.log("  'Stattdessen mit Telefonnummer verknüpfen' → Code eingeben.\n");
    } catch (e) {
      console.log(`  Konnte Pairing-Code nicht anfordern: ${e.message}`);
    }
  }

  return new Promise((resolvePromise) => {
    let done = false;
    const finish = (result) => {
      if (done) return;
      done = true;
      clearTimeout(timeout);
      resolvePromise(result);
    };
    const timeout = setTimeout(() => {
      sock.end();
      finish({ ok: false, msg: "Timeout (180s) — kein Code eingegeben" });
    }, 180000);

    sock.ev.on("connection.update", (update) => {
      const { connection, lastDisconnect } = update;

      if (connection === "open") {
        console.log("\n  Login erfolgreich!");
        console.log(`  Auth: ${AUTH_DIR}`);
        console.log("  Warte 10s damit Baileys creds.json fertig schreibt...");
        setTimeout(() => finish({ ok: true }), 10000);
      }

      if (connection === "close") {
        const err = lastDisconnect?.error;
        const code = err?.output?.statusCode;
        const msg = err?.message || "unbekannt";
        finish({ ok: false, code, msg });
      }
    });
  });
}

async function main() {
  let result = await loginOnce(true);

  if (!result.ok) {
    console.log(`Verbindung geschlossen (Code: ${result.code ?? "?"}, ${result.msg})`);

    if (result.code === 515) {
      // Normal restart after successful pairing — reconnect, no new code.
      console.log("Restart required — reconnecting...\n");
      result = await loginOnce(false);
      if (!result.ok) throw new Error(`Reconnect fehlgeschlagen: ${result.msg}`);
    } else if (result.code === 401 || result.code === 403) {
      console.log("Session ungültig — lösche und starte neu...\n");
      if (existsSync(AUTH_DIR)) rmSync(AUTH_DIR, { recursive: true, force: true });
      result = await loginOnce(true);
      if (!result.ok) throw new Error(`Neu-Login fehlgeschlagen: ${result.msg}`);
    } else {
      throw new Error(`Verbindung geschlossen: ${result.msg}`);
    }
  }

  const files = existsSync(AUTH_DIR) ? readdirSync(AUTH_DIR) : [];
  console.log(`Session-Dateien: ${files.length}`);
}

main()
  .then(() => process.exit(0))
  .catch((err) => {
    console.error("Error:", err.message);
    process.exit(1);
  });
