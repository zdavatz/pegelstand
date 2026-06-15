#!/usr/bin/env node
// WhatsApp login — render QR as PNG and open it in a window (in addition to
// printing the ASCII QR to the terminal). Usage: node login-qr.mjs [--force]

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
} from "@whiskeysockets/baileys";
import qrcodeTerminal from "qrcode-terminal";
import QRCode from "qrcode";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { rmSync, existsSync, readdirSync } from "fs";
import { spawn } from "child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const QR_PNG = "/tmp/wa-login-qr.png";
const logger = pino({ level: "silent" });

const force = process.argv.includes("--force");

if (force && existsSync(AUTH_DIR)) {
  console.log("Lösche alte Session (--force)...");
  rmSync(AUTH_DIR, { recursive: true, force: true });
}

let viewerOpened = false;
async function showQrWindow(qr) {
  // Big, high-margin PNG so phone cameras lock on quickly.
  await QRCode.toFile(QR_PNG, qr, { width: 600, margin: 4 });
  if (!viewerOpened) {
    viewerOpened = true;
    const viewer = spawn("feh", ["--auto-zoom", QR_PNG], {
      detached: true,
      stdio: "ignore",
    });
    viewer.on("error", () => {
      // feh missing — fall back to xdg-open
      spawn("xdg-open", [QR_PNG], { detached: true, stdio: "ignore" }).unref();
    });
    viewer.unref();
    console.log(`  QR-Code-Fenster geöffnet (${QR_PNG}).`);
  }
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
      finish({ ok: false, msg: "Timeout (180s) — kein QR-Code gescannt" });
    }, 180000);

    sock.ev.on("connection.update", (update) => {
      const { connection, qr, lastDisconnect } = update;

      if (qr) {
        console.log("\n  Scan this QR code with WhatsApp:");
        console.log("  (Settings > Linked Devices > Link a Device)\n");
        qrcodeTerminal.generate(qr, { small: true });
        showQrWindow(qr).catch((e) =>
          console.error("  QR-Fenster fehlgeschlagen:", e.message)
        );
      }

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
  let result = await loginOnce();

  if (!result.ok) {
    console.log(`Verbindung geschlossen (Code: ${result.code ?? "?"}, ${result.msg})`);

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
