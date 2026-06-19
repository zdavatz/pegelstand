#!/usr/bin/env node
// Read-only: connect with existing auth, capture history sync, find a contact
// by name filter and print their most recent messages. Sends nothing.
//
// Usage: node read-messages.mjs <nameFilter> [maxMessages]

import makeWASocket, {
  useMultiFileAuthState,
  makeCacheableSignalKeyStore,
  fetchLatestBaileysVersion,
  DisconnectReason,
} from "@whiskeysockets/baileys";
import pino from "pino";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const AUTH_DIR = resolve(__dirname, "auth");
const logger = pino({ level: "silent" });

const nameFilter = (process.argv[2] || "").toLowerCase();
const numberFilter = (process.argv[3] || "").replace(/\D/g, ""); // digits only
const MAX = parseInt(process.argv[4] || "30", 10);

const contactsByJid = new Map();   // jid -> best display name
const messagesByJid = new Map();   // jid -> array of {ts, fromMe, text}

function textOf(m) {
  const msg = m.message || {};
  return (
    msg.conversation ||
    msg.extendedTextMessage?.text ||
    msg.imageMessage?.caption ||
    msg.videoMessage?.caption ||
    msg.documentMessage?.caption ||
    (msg.documentMessage ? `[document: ${msg.documentMessage.fileName || ""}]` : "") ||
    (msg.imageMessage ? "[image]" : "") ||
    (msg.videoMessage ? "[video]" : "") ||
    (msg.audioMessage ? "[audio]" : "") ||
    (msg.stickerMessage ? "[sticker]" : "") ||
    (msg.reactionMessage ? `[reaction: ${msg.reactionMessage.text}]` : "") ||
    ""
  );
}

function recordContact(c) {
  if (!c?.id) return;
  const name = c.name || c.notify || c.verifiedName || "";
  if (name) {
    const prev = contactsByJid.get(c.id) || "";
    if (name.length > prev.length) contactsByJid.set(c.id, name);
  } else if (!contactsByJid.has(c.id)) {
    contactsByJid.set(c.id, "");
  }
}

function recordMessage(m) {
  const jid = m.key?.remoteJid;
  if (!jid || jid === "status@broadcast") return;
  const text = textOf(m);
  const ts = Number(m.messageTimestamp) || 0;
  if (!messagesByJid.has(jid)) messagesByJid.set(jid, []);
  messagesByJid.get(jid).push({
    ts,
    fromMe: !!m.key?.fromMe,
    pushName: m.pushName || "",
    text,
  });
}

async function main() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const { version } = await fetchLatestBaileysVersion();

  const sock = makeWASocket({
    version,
    logger,
    auth: {
      creds: state.creds,
      keys: makeCacheableSignalKeyStore(state.keys, logger),
    },
    markOnlineOnConnect: false,
    syncFullHistory: true,
  });

  sock.ev.on("creds.update", saveCreds);

  sock.ev.on("contacts.upsert", (cs) => cs.forEach(recordContact));
  sock.ev.on("contacts.update", (cs) => cs.forEach(recordContact));

  sock.ev.on("chats.upsert", (chats) =>
    chats.forEach((c) => {
      if (c.name) recordContact({ id: c.id, name: c.name });
    })
  );

  let histEvents = 0;
  sock.ev.on("messaging-history.set", ({ chats, contacts, messages, progress }) => {
    histEvents++;
    console.error(
      `[history.set #${histEvents}] chats=${chats?.length || 0} contacts=${contacts?.length || 0} messages=${messages?.length || 0} progress=${progress}`
    );
    (contacts || []).forEach(recordContact);
    (chats || []).forEach((c) => c?.name && recordContact({ id: c.id, name: c.name }));
    (messages || []).forEach(recordMessage);
  });

  sock.ev.on("messages.upsert", ({ messages }) => {
    (messages || []).forEach((m) => {
      if (m.pushName && m.key?.remoteJid && !m.key.fromMe) {
        recordContact({ id: m.key.remoteJid, notify: m.pushName });
      }
      recordMessage(m);
    });
  });

  // finalize early only once history reports complete
  sock.ev.on("messaging-history.set", ({ progress }) => {
    if (progress != null && progress >= 100) setTimeout(finishAndReport, 3000);
  });

  sock.ev.on("connection.update", (u) => {
    const { connection, lastDisconnect } = u;
    if (connection === "open") {
      console.error("[connected] waiting for history sync…");
      // hard ceiling only; do not report early so history chunks can arrive
      setTimeout(finishAndReport, 40000);
    }
    if (connection === "close") {
      const code = lastDisconnect?.error?.output?.statusCode;
      if (code === DisconnectReason.loggedOut) {
        console.error("[logged out] auth invalid");
        process.exit(2);
      }
    }
  });

  let reported = false;
  function finishAndReport() {
    if (reported) return;
    reported = true;

    // resolve matching jids by name filter
    const matches = [];
    const allJids = new Set([...contactsByJid.keys(), ...messagesByJid.keys()]);
    for (const jid of allJids) {
      if (jid.endsWith("@g.us")) continue; // skip groups for a person filter
      const name = contactsByJid.get(jid) || "";
      const numHit = numberFilter && jid.replace(/\D/g, "").includes(numberFilter);
      const nameHit = nameFilter && name.toLowerCase().includes(nameFilter);
      const wildcard = !nameFilter && !numberFilter;
      if (numHit || nameHit || wildcard) {
        if (name || messagesByJid.has(jid)) matches.push({ jid, name });
      }
    }
    // also match by pushName seen in messages
    for (const [jid, msgs] of messagesByJid) {
      if (jid.endsWith("@g.us")) continue;
      if (matches.find((m) => m.jid === jid)) continue;
      const pn = msgs.find((x) => x.pushName)?.pushName || "";
      if (nameFilter && pn.toLowerCase().includes(nameFilter)) {
        matches.push({ jid, name: pn });
      }
    }

    const out = { me: state.creds?.me, matches: [] };
    for (const { jid, name } of matches) {
      const msgs = (messagesByJid.get(jid) || [])
        .filter((m) => m.text)
        .sort((a, b) => a.ts - b.ts)
        .slice(-MAX);
      out.matches.push({ jid, name, count: msgs.length, messages: msgs });
    }
    // if no name match, dump a directory of known person-chats to help identify
    if (matches.length === 0) {
      out.directory = [];
      for (const [jid, msgs] of messagesByJid) {
        if (jid.endsWith("@g.us")) continue;
        const pn = msgs.find((x) => x.pushName)?.pushName || contactsByJid.get(jid) || "";
        out.directory.push({ jid, name: pn, msgCount: msgs.length });
      }
      out.directory.sort((a, b) => b.msgCount - a.msgCount);
      out.directory = out.directory.slice(0, 40);
    }

    console.log(JSON.stringify(out, null, 2));
    sock.end();
    setTimeout(() => process.exit(0), 500);
  }
}

main().catch((e) => {
  console.error("fatal:", e?.message || e);
  process.exit(1);
});
