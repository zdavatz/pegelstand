#!/usr/bin/env python3
"""Create a Google Form via the Forms REST API, authenticated AS THE USER
(zdavatz@gmail.com) with a 3-legged OAuth loopback flow.

Why not the service account? A Form is a Drive file and the service account
has no consumer-Drive storage, so forms.create returns 500. The form must be
created by a real user, so it lands in that user's Drive (owned by them).

Requires a Desktop OAuth client downloaded to whatsapp/google-oauth-client.json.
The refresh token is cached in whatsapp/google-oauth-token.json (gitignored),
so subsequent runs need no re-login until it's revoked.

Usage:
    python3 scripts/create_event_form_oauth.py [--relogin]
"""
import base64
import http.server
import json
import secrets
import sys
import threading
import urllib.parse
import urllib.request
import urllib.error
import webbrowser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLIENT_PATH = ROOT / "whatsapp" / "google-oauth-client.json"
TOKEN_PATH = ROOT / "whatsapp" / "google-oauth-token.json"

# Superset of scopes we'll need across the whole feature, requested once so we
# never have to re-consent: create the form, read its responses, send mail.
SCOPES = [
    "https://www.googleapis.com/auth/forms.body",
    "https://www.googleapis.com/auth/forms.responses.readonly",
    "https://www.googleapis.com/auth/gmail.send",
]

FORM_TITLE = "Hitachi Pumpfoil Event"
EVENT_WHEN = "Mittwoch, 16.9.2026, 18:00 Uhr"
FORM_DESCRIPTION = (
    FORM_TITLE + " — " + EVENT_WHEN + ".\n"
    "Bitte fülle das Formular aus, um dich anzumelden. "
    "Du erhältst eine Bestätigung per E-Mail und WhatsApp."
)
QUESTIONS = ["Vorname", "Nachname", "Email", "Mobile", "Gewicht", "Alter", "Grösse"]
AUTH_ENDPOINT = "https://accounts.google.com/o/oauth2/v2/auth"
TOKEN_ENDPOINT = "https://oauth2.googleapis.com/token"


def load_client():
    raw = json.loads(CLIENT_PATH.read_text())
    node = raw.get("installed") or raw.get("web") or raw
    return node["client_id"], node["client_secret"]


def post_form(url, fields):
    data = urllib.parse.urlencode(fields).encode()
    req = urllib.request.Request(url, data=data)
    with urllib.request.urlopen(req) as r:
        return json.load(r)


def login():
    """Loopback OAuth: spin up a localhost server, open consent, capture code."""
    client_id, client_secret = load_client()
    holder = {}

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            qs = urllib.parse.urlparse(self.path).query
            holder.update(urllib.parse.parse_qs(qs))
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.end_headers()
            self.wfile.write(
                "<h2>Login OK — du kannst dieses Fenster schliessen.</h2>"
                .encode("utf-8"))

        def log_message(self, *a):
            pass

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    port = server.server_address[1]
    redirect_uri = f"http://127.0.0.1:{port}/"
    state = secrets.token_urlsafe(16)

    auth_url = AUTH_ENDPOINT + "?" + urllib.parse.urlencode({
        "client_id": client_id,
        "redirect_uri": redirect_uri,
        "response_type": "code",
        "scope": " ".join(SCOPES),
        "access_type": "offline",
        "prompt": "consent",
        "state": state,
    })

    print("\nÖffne diese URL im Browser (eingeloggt als zdavatz@gmail.com)")
    print("und klicke ALLOW:\n")
    print("  " + auth_url + "\n")
    try:
        webbrowser.open(auth_url)
    except Exception:
        pass

    t = threading.Thread(target=server.handle_request)
    t.start()
    t.join(timeout=300)
    server.server_close()

    if "code" not in holder:
        raise SystemExit("Kein Code empfangen (Timeout oder abgebrochen).")
    if holder.get("state", [None])[0] != state:
        raise SystemExit("State mismatch — Abbruch.")

    tok = post_form(TOKEN_ENDPOINT, {
        "code": holder["code"][0],
        "client_id": client_id,
        "client_secret": client_secret,
        "redirect_uri": redirect_uri,
        "grant_type": "authorization_code",
    })
    TOKEN_PATH.write_text(json.dumps(tok, indent=2))
    print(f"Token gespeichert: {TOKEN_PATH}")
    return tok["access_token"]


def access_token(relogin=False):
    if relogin or not TOKEN_PATH.exists():
        return login()
    tok = json.loads(TOKEN_PATH.read_text())
    if "refresh_token" not in tok:
        return login()
    client_id, client_secret = load_client()
    try:
        fresh = post_form(TOKEN_ENDPOINT, {
            "client_id": client_id,
            "client_secret": client_secret,
            "refresh_token": tok["refresh_token"],
            "grant_type": "refresh_token",
        })
        tok.update(fresh)
        TOKEN_PATH.write_text(json.dumps(tok, indent=2))
        return tok["access_token"]
    except urllib.error.HTTPError:
        return login()


def api(method, url, token, payload=None):
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {token}")
    if data is not None:
        req.add_header("Content-Type", "application/json")
    try:
        with urllib.request.urlopen(req) as r:
            txt = r.read().decode()
            return json.loads(txt) if txt else {}
    except urllib.error.HTTPError as e:
        print(f"\nAPI ERROR {e.code} {method} {url}:\n{e.read().decode()}\n",
              file=sys.stderr)
        raise


def main():
    if not CLIENT_PATH.exists():
        raise SystemExit(f"Fehlt: {CLIENT_PATH}\n"
                         "Lade den Desktop-OAuth-Client als diese Datei herunter.")
    token = access_token(relogin="--relogin" in sys.argv)

    form = api("POST", "https://forms.googleapis.com/v1/forms", token,
               {"info": {"title": FORM_TITLE, "documentTitle": FORM_TITLE}})
    form_id = form["formId"]
    print(f"Form created: {form_id}")

    requests = [{
        "updateFormInfo": {"info": {"description": FORM_DESCRIPTION},
                           "updateMask": "description"}
    }]
    for idx, title in enumerate(QUESTIONS):
        requests.append({"createItem": {
            "item": {"title": title, "questionItem": {
                "question": {"required": True,
                             "textQuestion": {"paragraph": False}}}},
            "location": {"index": idx}}})
    api("POST", f"https://forms.googleapis.com/v1/forms/{form_id}:batchUpdate",
        token, {"requests": requests})
    print(f"Added description + {len(QUESTIONS)} questions.")

    info = api("GET", f"https://forms.googleapis.com/v1/forms/{form_id}", token)
    print("\n=== DONE ===")
    print(f"Form ID:       {form_id}")
    print(f"Responder URL: {info.get('responderUri')}")
    print(f"Edit URL:      https://docs.google.com/forms/d/{form_id}/edit")


if __name__ == "__main__":
    main()
