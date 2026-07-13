# E-mail setup for `sync-contacts --force-email` (and the WhatsApp→e-mail fallback)

The sender auto-detects its transport (`gmail::Mailer::autodetect`):

1. **SMTP + Gmail App Password** — used if `~/.config/pegelstand/gmail-app-password.txt`
   (or `$PEGELSTAND_GMAIL_APP_PASSWORD`) exists. **Recommended** — simplest, fully
   automatic, no expiry.
2. **Gmail API (OAuth)** — fallback if no app password: dedicated OAuth client
   (`gmail_auth` → `whatsapp/google-oauth-token.json`), else gcloud ADC.

Why not gcloud/ADC directly: Google now blocks the shared gcloud OAuth client from
the `gmail.send` scope (`cloud-platform required` / "Diese App ist blockiert").

---

## Option 1 — SMTP App Password (recommended, ~2 min, no expiry)

1. **Enable 2-Step Verification** on `zdavatz@gmail.com` (if not already):
   myaccount.google.com → Security → 2-Step Verification.
2. **Create an App Password**: myaccount.google.com → Security → search "App passwords"
   → create one (name it e.g. `pegelstand`). Google shows 16 chars as `abcd efgh ijkl mnop`.
3. **Save it** (spaces are fine — they're stripped on read):
   ```bash
   mkdir -p ~/.config/pegelstand
   printf '%s\n' 'abcd efgh ijkl mnop' > ~/.config/pegelstand/gmail-app-password.txt
   chmod 600 ~/.config/pegelstand/gmail-app-password.txt
   ```
4. Test:
   ```bash
   ./target/release/pegelstand sync-contacts --force-email
   # → "Transport: SMTP (App-Passwort)"  then  "✓ … E-Mail gesendet"
   ```

That's it — indefinite, no maintenance. (Sends via `smtp.gmail.com`, cap 500 mails/day.)

**Security note:** an app password also permits IMAP read access to the mailbox, so
keep the file `chmod 600` and never commit it (it lives outside the repo under `~/`).
To revoke: myaccount.google.com → App passwords → delete.

---

## Option 2 — Gmail API OAuth client (only if you prefer OAuth over an app password)

`gmail.send` is a **sensitive** (not restricted) scope → no CASA security audit, but:
unverified/Testing tokens expire after **7 days**; indefinite tokens require Google's
sensitive-scope verification (privacy-policy URL + app homepage + review, ~days–weeks).

1. Google Cloud → project `pegelstand` → enable Gmail API.
2. Create an OAuth **Desktop** client; add `zdavatz@gmail.com` as a test user; add scope
   `.../auth/gmail.send`. Download the client JSON → `whatsapp/google-oauth-client.json`.
3. `cargo run --release --bin gmail_auth` → open the URL, approve → writes
   `whatsapp/google-oauth-token.json`.
4. For an indefinite token: publish the app + complete sensitive-scope verification,
   then re-run `gmail_auth`.

Both `whatsapp/google-oauth-client.json` and `google-oauth-token.json` are gitignored.
