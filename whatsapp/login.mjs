#!/usr/bin/env node
// WhatsApp login — scan QR code, save session
// Usage: node login.mjs [--force]

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
} from "@whiskeysockets/baileys";
import qrcode from "qrcode-terminal";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { rmSync, existsSync, readdirSync } from "fs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const force = process.argv.includes("--force");

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
    browser: ["Pegelstand", "CLI", "1.0"],
    syncFullHistory: false,
    markOnlineOnConnect: false,
  });

  sock.ev.on("creds.update", saveCreds);
  return sock;
}

async function loginOnce() {
  const sock = await startSocket();

  return new Promise((resolve) => {
    let done = false;
    const finish = (result) => {
      if (done) return;
      done = true;
      clearTimeout(timeout);
      resolve(result);
    };
    const timeout = setTimeout(() => {
      sock.end();
      finish({ ok: false, msg: "Timeout (120s) — kein QR-Code gescannt" });
    }, 120000);

    sock.ev.on("connection.update", (update) => {
      const { connection, qr, lastDisconnect } = update;

      if (qr) {
        console.log("\n  Scan this QR code with WhatsApp:");
        console.log("  (Settings > Linked Devices > Link a Device)\n");
        qrcode.generate(qr, { small: true });
      }

      if (connection === "open") {
        console.log("\n  Login erfolgreich! Session gespeichert.");
        console.log(`  Auth: ${AUTH_DIR}\n`);
        finish({ ok: true });
        setTimeout(() => sock.end(), 500);
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
  let result = await loginOnce();

  if (!result.ok) {
    console.log(`Verbindung geschlossen (Code: ${result.code ?? "?"}, ${result.msg})`);

    // 401 = loggedOut (session invalid on phone side) — wipe and retry with fresh QR
    // 515 = restartRequired (normal after first login)
    if (result.code === 401 || result.code === 403) {
      console.log("Session auf Telefon abgelaufen/ungültig — lösche und starte Neu-Login...\n");
      if (existsSync(AUTH_DIR)) {
        rmSync(AUTH_DIR, { recursive: true, force: true });
      }
      result = await loginOnce();
      if (!result.ok) throw new Error(`Neu-Login fehlgeschlagen: ${result.msg}`);
    } else if (result.code === 515) {
      console.log("Restart required — reconnecting...\n");
      result = await loginOnce();
      if (!result.ok) throw new Error(`Reconnect fehlgeschlagen: ${result.msg}`);
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
