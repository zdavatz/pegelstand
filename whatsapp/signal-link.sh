#!/usr/bin/env bash
# Link signal-cli as a secondary device of Zeno's Signal account (one-time),
# for the Signal fallback in check-and-send.mjs (numbers not on WhatsApp).
# Shows the link QR as PNG (/tmp/signal-link-qr.png, feh window) and in the
# terminal; scan it in Signal → Einstellungen → Verknüpfte Geräte → "+".
# On success the account number goes to ~/.config/pegelstand/signal-account.txt.
set -euo pipefail
CLI=${SIGNAL_CLI:-$HOME/.local/bin/signal-cli}
[ -x "$CLI" ] || CLI=signal-cli
PNG=/tmp/signal-link-qr.png
OUT=$(mktemp)
trap 'rm -f "$OUT"; [ -n "${FEH:-}" ] && kill "$FEH" 2>/dev/null || true' EXIT

"$CLI" link -n "Pegelstand" >"$OUT" 2>&1 &
PID=$!
for _ in $(seq 60); do grep -q '^sgnl://' "$OUT" && break; sleep 1; done
URI=$(grep -m1 '^sgnl://' "$OUT") || { cat "$OUT"; exit 1; }
qrencode -o "$PNG" -s 10 "$URI"
qrencode -t ANSIUTF8 "$URI"
echo "QR: $PNG — Signal → Einstellungen → Verknüpfte Geräte → +"
DISPLAY=${DISPLAY:-:0} feh --auto-zoom --title "Signal QR" "$PNG" >/dev/null 2>&1 & FEH=$!

wait "$PID" || { echo "signal-cli link fehlgeschlagen:"; grep -v '^sgnl://' "$OUT"; exit 1; }
cat "$OUT"
ACC=$(grep -oE '\+[0-9]{8,15}' "$OUT" | tail -1 || true)
[ -n "$ACC" ] || ACC=$("$CLI" listAccounts 2>/dev/null | grep -oE '\+[0-9]{8,15}' | head -1 || true)
[ -n "$ACC" ] || { echo "Kein Konto erkannt — Link fehlgeschlagen?"; exit 1; }
mkdir -p ~/.config/pegelstand
echo "$ACC" > ~/.config/pegelstand/signal-account.txt
chmod 600 ~/.config/pegelstand/signal-account.txt
"$CLI" -a "$ACC" receive -t 5 >/dev/null 2>&1 || true  # initial sync
echo "Verknüpft — Konto in ~/.config/pegelstand/signal-account.txt gespeichert."
