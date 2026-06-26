# Pegelstand

CLI-Tool zur Abfrage von Gewässerdaten (Pegel, Temperatur, Wind, Wellen) für Pumpfoilen und Wingfoilen.

Standorte: Zürichsee, Silvaplana, Neuenburgersee, Urnersee, Greifensee, Ermioni (Griechenland).

## Datenquellen

- **BAFU / api.existenz.ch** — Pegelstände, Abfluss, Fluss-Temperaturen (237+ Stationen)
- **MeteoSwiss / SwissMetNet (SMN)** — Wind, Temperatur, Druck, Niederschlag, Strahlung (via api.existenz.ch)
- **InfluxDB (api.existenz.ch)** — Historische Daten ab 2001 (Hydro + SMN)
- **Wasserschutzpolizei Zürich (tecdottir)** — Zürichsee Wassertemperatur, Wetter (seit 2007)
- **Open-Meteo** — Wind, Temperatur, Wellen für Ermioni/Griechenland (Modell-Daten, kein API-Key)
- **Poseidon/HCMR** — Saronikos-Boje (Wind, Wellen, Wassertemp) — OAuth2-Registrierung nötig

## Installation

```bash
cargo build --release
```

## Befehle

### Messstationen suchen

```bash
pegelstand locations                    # Alle Stationen
pegelstand locations --filter Zürich    # Nach Name/Gewässer filtern
```

### Verfügbare Parameter

```bash
pegelstand parameters
```

### Aktuelle Messwerte

```bash
pegelstand latest -l 2209              # Zürichsee
pegelstand latest -l 2209,2099 -p height,temperature,flow
```

### Zürichsee-Pegel mit Reglement-Bewertung

```bash
pegelstand zurichsee
```

Zeigt den aktuellen Pegel und bewertet ihn anhand des Reglements 1977 für die Regulierung der Wasserstände des Zürichsees (Regulierlinie, Abflussgrenze 405.90 m).

### Historische Daten (InfluxDB)

```bash
pegelstand history -l 2209 -p height -r 3mo -e 1d    # Zürichsee, 3 Monate, täglich
pegelstand history -l 2135 -p temperature -r 1y -e 7d # Aare Bern, 1 Jahr, wöchentlich
pegelstand history -l 2143 -p flow -r 30d -e 1h       # Rhein, 30 Tage, stündlich
```

### Fluss-Temperaturen (alle Stationen)

```bash
pegelstand temperaturen                    # Sortiert nach Temperatur
pegelstand temperaturen --filter Aare      # Nur Aare-Stationen
pegelstand temperaturen --sort name        # Nach Name sortiert
pegelstand temperaturen --sort gewaesser   # Nach Gewässer sortiert
```

### Zürichsee Wetter &amp; Wassertemperatur (Wasserschutzpolizei)

Kombiniert automatisch beide Stationen: **Tiefenbrunnen (T)** und **Mythenquai (M)**.

- T: Wassertemp, Lufttemp, Windchill, Taupunkt, Feuchtigkeit, Wind, Böen, Beaufort, Windrichtung, Luftdruck
- M: Niederschlag, Sonnenstrahlung, Pegel

```bash
pegelstand seetemperatur --aktuell                     # Alle Werte (T+M kombiniert)
pegelstand seetemperatur                               # 3 Monate Tageswerte
pegelstand seetemperatur --datum 2026-04-08            # Alle 10-Min-Werte eines Tages (T+M)
pegelstand seetemperatur --start 2025-06-01 --end 2025-09-01  # Eigener Zeitraum
```

### Silvaplana — Wind, Wetter & Pegel

Daten von MeteoSwiss Station SIA (Segl-Maria, ~3 km vom Silvaplanersee) + BAFU Pegel 2073.

```bash
pegelstand silvaplana --aktuell                        # Aktuell: Wind, Temp, Druck, Strahlung, Pegel
pegelstand silvaplana --datum 2026-04-08               # Alle 10-Min-Werte eines Tages
pegelstand silvaplana                                  # 30-Tage-Übersicht (Tagesmax Wind/Böen)
pegelstand silvaplana --start 2025-06-01 --end 2025-08-31  # Eigener Zeitraum
```

### Sihlsee — Wind & Wetter

Daten von MeteoSwiss Station EIN (Einsiedeln, 1.8 km vom Sihlsee). Kein See-Pegel (Stausee Axpo/EWS).

```bash
pegelstand sihlsee --aktuell                           # Aktuell: Wind, Temp, Druck, Strahlung
pegelstand sihlsee --datum 2026-04-08                  # Alle 10-Min-Werte eines Tages
pegelstand sihlsee                                     # 30-Tage-Übersicht
```

### Ermioni (Griechenland) — Wind, Wetter & Wellen

Daten von Open-Meteo (Modell) + Open-Meteo Marine (Wellen). Optional: Poseidon/HCMR Saronikos-Boje.

```bash
pegelstand ermioni --aktuell                           # Aktuell: Wind, Temp, Böen
pegelstand ermioni                                     # Letzte 7 Tage (stündlich, Konsole)
pegelstand ermioni --start 2025-07-01 --end 2025-07-31 # Eigener Zeitraum
pegelstand ermioni --start 2026-04-10 --end 2026-04-17 --png           # SVG + PNG (2x Retina)
pegelstand ermioni --start 2026-04-10 --end 2026-04-17 --png --whatsapp "GROUP_JID@g.us"
pegelstand ermioni --start 2026-04-17 --end 2026-04-22 --png --whatsapp "GROUP_JID@g.us"  # 4 Tage + 1 Tag Forecast
pegelstand ermioni --start 2026-04-25 --end 2026-04-30 --png --bg ~/Pictures/foto.heic   # mit Hintergrundbild
```

5 SVG-Charts: Wind & Böen, Windrichtung (0–360°), Lufttemperatur, Wellenhöhe, Luftdruck. Ausgabe: SVG in `svg/`, PNG in `png/`. Am Ende jeder Linie wird der aktuelle Wert numerisch angezeigt.

Archive- vs. Forecast-API: Archive wird nur verwendet, wenn **beide** Daten älter als 2 Tage sind. Gemischte Zeiträume (Vergangenheit + Heute/Zukunft) laufen über die Forecast-API, die auch kürzlich Vergangenes per `start_date`/`end_date` liefert.

Poseidon/HCMR (Palea Fokea, Saronischer Golf): Registrieren unter https://auth.poseidon.hcmr.gr/auth/register/, Daten via https://apps.poseidon.hcmr.gr/webapp/poseidon_db/ (NetCDF per E-Mail).

**Station Palea Fokea** (37.72°N, 23.95°E, ~50 km NW von Ermioni):
- Lufttemperatur, Windgeschwindigkeit, Windrichtung, Luftdruck, Feuchtigkeit, Meeresspiegel
- 5-Minuten-Intervall, echte Messdaten (nicht Modell)

### HTML-Report generieren

```bash
# Interaktiver Report (Chart.js, für Browser)
pegelstand report --start 2026-03-25 --end 2026-03-26

# SVG-Report (kein JavaScript, für WhatsApp/Mail/Offline)
pegelstand report --start 2026-03-25 --end 2026-03-26 --svg

# Seen-Reports (Wind, Temp, Strahlung — MeteoSwiss)
pegelstand report --start 2025-05-01 --end 2025-09-30 --silvaplana      # Wingfoilen
pegelstand report --start 2025-05-01 --end 2025-09-30 --neuenburgersee  # Downwinden
pegelstand report --start 2025-05-01 --end 2025-09-30 --urnersee        # Föhn
pegelstand report --start 2025-05-01 --end 2025-09-30 --greifensee     # Pumpfoilen
pegelstand report --start 2025-05-01 --end 2025-09-30 --sihlsee      # Pumpfoilen
pegelstand report --start 2025-05-01 --end 2025-09-30 --ermioni       # Wingfoilen GR

# Eigene Ausgabedatei
pegelstand report --start 2026-03-25 --end 2026-03-26 --svg -o bericht.html
```

**Seen-Reports** (Silvaplana, Neuenburgersee, Urnersee):

| See | MeteoSwiss | BAFU Pegel | Typisch für |
|-----|------------|------------|-------------|
| `--silvaplana` | SIA (Segl-Maria) | 2073 | Wingfoilen (Maloja-Wind) |
| `--neuenburgersee` | PAY (Payerne) | 2154 (Grandson) | Downwinden |
| `--urnersee` | ALT (Altdorf) | 2025 (Brunnen) | Föhn-Sessions |
| `--greifensee` | PFA (Pfaffikon ZH) | 2082 | Pumpfoilen |
| `--sihlsee` | EIN (Einsiedeln) | 2609 (Alp Zufluss) | Pumpfoilen |
| `--ermioni` | Open-Meteo | — | Wingfoilen GR |

Alle Reports enthalten:
- Charts: Wind/Böen, Windrichtung, Temperatur, Luftdruck, Sonnenstrahlung
- Automatischer InfluxDB-Fallback für Daten älter als ~30 Tage (stündlich aggregiert)
- Klickbare Google Maps Links zu allen Messstationen (neuer Tab)
- Webcam-Links pro Standort (neuer Tab)
- Neuen See hinzufügen: nur ein `LakeConfig`-Eintrag

Zürichsee-Report enthält:
- Statistik-Karten (Min/Max Wassertemp, Windchill, Böen, Beaufort, Luftdruck)
- Charts: Temperaturverlauf, Wind/Böen, Windrichtung, Luftdruck, Pegel
- Vollständige Datentabelle (alle 10-Minuten-Messwerte)
- Quellenangabe pro Feld (T = Tiefenbrunnen, M = Mythenquai)
- Klickbare Google Maps Links zu allen Messstationen

| Modus | Dateigrösse | JavaScript | WhatsApp | Interaktiv |
|-------|-------------|------------|----------|------------|
| Chart.js | ~244 KB | ja | nein | ja (Hover) |
| `--svg` | ~124 KB | nein | ja | nein |

### Standalone SVG (Zürichsee)

Reine SVG-Datei mit Temperatur, Pegelstand, Wind/Böen, Windrichtung und Luftdruck — kein HTML, kein JavaScript. Ideal für Einbettung, WhatsApp, E-Mail.

```bash
pegelstand svg                                             # Letzte 5 Tage (Standard)
pegelstand svg --start 2026-04-01 --end 2026-04-10         # Eigener Zeitraum
pegelstand svg -o mein_chart.svg                           # Eigene Ausgabedatei
pegelstand svg --start 2026-04-10 --end 2026-04-11 --png   # SVG + PNG (für WhatsApp)
pegelstand svg --start 2026-04-10 --end 2026-04-11 --png --whatsapp "GROUP_JID@g.us"  # PNG an WhatsApp-Gruppe senden
pegelstand svg --start 2026-04-25 --end 2026-04-30 --png --bg ~/Pictures/foto.heic  # mit Hintergrundbild
```

Ausgabe: SVG im `svg/`-Verzeichnis, PNG im `png/`-Verzeichnis. PNG wird mit 2x Auflösung (Retina) via `resvg` gerendert. Datumsformat: dd.mm.yyyy. Quellen: Tiefenbrunnen (T) + Mythenquai (M).

5 Charts: Temperatur (Wasser + Luft), Pegelstand, Wind & Böen, Windrichtung (0–360°, Punkte), Luftdruck. X-Achsen-Labels: erstes Label linksbündig, letztes rechtsbündig — kein Abschneiden am SVG-Rand. Am Ende jeder Linie wird der aktuelle Wert numerisch angezeigt.

`--bg <pfad>` (`svg` und `ermioni`) bettet ein Bild als Diagramm-Hintergrund ein (HEIC/JPEG/PNG/WebP, longest-side auf 1500px verkleinert, opacity 0.25). Konvertierung läuft über macOS `qlmanage`, das die HEIC-`irot`-Box korrekt anwendet (sips ignoriert sie und liefert sideways-rotierte Bilder).

### Standalone SVG (Palea Fokea / Poseidon)

SVG-Chart aus NetCDF-Daten der Poseidon/HCMR-Station Palea Fokea (Saronischer Golf). Pure Rust NetCDF3-Parser, keine C-Abhängigkeiten.

```bash
pegelstand paleafokea                                      # Neueste .nc Datei aus poseidon_data/
pegelstand paleafokea --file poseidon_data/meine_datei.nc  # Bestimmte Datei
pegelstand paleafokea --png                                # SVG + PNG (2x Retina)
pegelstand paleafokea --png --whatsapp "GROUP_JID@g.us"    # PNG an WhatsApp senden
```

5 Charts: Lufttemperatur, Meeresspiegel, Windgeschwindigkeit, Windrichtung (0–360°), Luftdruck. Datenquelle: NetCDF-Dateien von [POSEIDON/HCMR](https://apps.poseidon.hcmr.gr/webapp/poseidon_db/).

### Rechtsgrundlagen-Dossier (Pumpfoilen am Zürichsee)

Eigenständiges Programm `rechtsgrundlagen`, das ein PDF mit den anwendbaren Rechtsgrundlagen für das Pumpfoilen auf dem Zürichsee erzeugt — vom Bundesrecht (BSG, Binnenschifffahrtsverordnung BSV) über das interkantonale Recht und das kantonale Recht (inkl. neuem Wassergesetz, in Kraft ab 1.6.2026) bis zum städtischen Recht, samt der «gelben Zone» (Sperrfläche) und der Ausnahmefrage. Begleitdokument zur Schriftlichen Anfrage GR Nr. 2026/250.

```bash
cargo run --release --bin rechtsgrundlagen   # → recht/Rechtsgrundlagen_Pumpfoiling_Zuerichsee.pdf
FONT_DIR=/usr/share/fonts/dejavu cargo run --release --bin rechtsgrundlagen
```

Reines Rust ohne Chrome (PDF via `genpdf`, DejaVu Sans eingebettet). Alle 17 Quell- und Gesetzes-URLs (fedlex, zh.ch/zhlex, stadt-zuerich.ch, das Geschäft + die Anfrage) sind **anklickbar** — die Links werden nachträglich mit `lopdf` als `/Link`-Annotationen über die jeweilige URL-Zeile gelegt, da `genpdf` selbst keine Hyperlinks setzt.

#### WhatsApp-Integration

Das PNG kann direkt an eine WhatsApp-Gruppe gesendet werden via [Baileys](https://github.com/WhiskeySockets/Baileys) (WhatsApp Web Protokoll, Node.js ≥ 22).

```bash
pegelstand whatsapp login                                    # 1x QR-Code scannen
pegelstand whatsapp groups                                   # Gruppen-JIDs anzeigen
pegelstand whatsapp leave "GROUP_JID@g.us"                   # Gruppe verlassen
pegelstand svg --png --whatsapp "GROUP_JID@g.us"             # Generieren + senden
node whatsapp/send-doc.mjs <jid-oder-nummer> <pfad> [caption]  # beliebige Datei (PDF/CSV/...) senden
```

Beim ersten `login` wird ein QR-Code im Terminal angezeigt — mit WhatsApp scannen (Einstellungen → Verknüpfte Geräte). Die Session wird in `whatsapp/auth/` gespeichert. npm-Abhängigkeiten werden automatisch installiert.

`node whatsapp/login-qr.mjs [--force]` ist eine Login-Variante, die den QR-Code zusätzlich als PNG (`/tmp/wa-login-qr.png`) rendert und in einem Fenster (`feh`, Fallback `xdg-open`) öffnet — praktisch, wenn der ASCII-QR im Terminal schwer scannbar ist. `--force` löscht die alte Session zuerst.

`send-doc.mjs` akzeptiert sowohl Group-JIDs (`...@g.us`) als auch reine Telefonnummern (`41787496544` → automatisch zu `41787496544@s.whatsapp.net`). Bilder werden als `image:` gesendet, alles andere als `document:`. Längeres 5-Min-Verbindungs-Timeout und 10-Sek-Exit-Delay nach dem Send, damit der asynchrone `creds.json`-Write fertig ist.

### Pump Tsüri — Willkommens-Nachrichten an neue Pumper

Liest ein Google-Formular, filtert neue Einträge (Diff gegen lokale SQLite-DB) und schickt jedem neuen Eintrag eine personalisierte WhatsApp-Nachricht. Anmeldungen, die **nicht auf WhatsApp** sind, bekommen die Nachricht stattdessen automatisch **per E-Mail** (siehe unten). Zwei vorkonfigurierte Varianten mit jeweils eigener DB:

| Variante | Sheet | DB | Nachricht | PNG |
|----------|-------|------|-----------|-----|
| (Standard) | Pump-Tsüri Anmeldung | `whatsapp/contacts.db` | "Hallo {first}! Willkommen bei Pump Tsüri! Anbei die Wassertemperatur vom Zürichsee der letzten 3 Tage." | 3-Tage Zürichsee-Wassertemperatur |
| `pp` (Power Pumper) | 1-Minute-Achievement Sheet | `whatsapp/contacts_pp.db` | "Herzliche Gratulation zur erreichten Minute \"{first}\"! Bitte twinte mir noch CHF 10.- dann legen ich dir die Mütze auf die Post. Gruss Zeno" | — |
| `build` (Build & Pump Event) | Build-&-Pump-Event Anmeldung | `whatsapp/contacts_build.db` | "Welcome to the build and pump event {first}." | — |

```bash
pegelstand welcome --dry-run          # zeigt, was getan würde — keine Sends, kein DB-Insert
pegelstand welcome                    # Pumper-Variante: PNG + Willkommen
pegelstand welcome pp                 # Power-Pumper-Variante: Twint/Mütze-Nachricht
pegelstand welcome build              # Build-&-Pump-Event-Variante: Text-only Welcome
pegelstand welcome --mark-existing    # Alle aktuellen Einträge als 'schon begrüsst' markieren, ohne Versand (Backfill)
pegelstand welcome pp --mark-existing # Dito für die Power-Pumper-DB
pegelstand welcome pp --regen-docs                  # OneDrive-Mütze-Dokumente für ALLE registrierten pp-Kontakte neu erzeugen (kein Versand)
pegelstand welcome pp --regen-docs --regen-rows 132 # Nur für bestimmte Zeilen-Indizes
pegelstand sync-contacts              # Voller Name; "welcome" ist nur ein Alias
```

Flags (alle optional, Preset liefert sinnvolle Defaults):
- `--sheet <URL>` — anderes Sheet
- `--db <name>` — DB-Dateiname unter `whatsapp/` (z.B. `contacts_test.db` für einen Test-Lauf)
- `--welcome "..."` — eigener Text (Platzhalter `{first}`, `{last}`, `{name}`)
- `--mobile-col C --first-col J --last-col D` — Spalten-Buchstaben (jede Variante hat eigene Defaults)
- `--days N` — Tage zurück für den PNG-Chart (Standard 3, nur Pumper-Variante)
- `--no-image` — PNG-Versand überspringen
- `--cc 41` — Default-Ländercode für Nummern ohne `+`
- `--mark-existing` — kein Versand; alle aktuell offenen Einträge werden in die `contacts`-Tabelle geschrieben
- `--dry-run` — keine WhatsApp-Aufrufe, keine `contacts`-Inserts (Submissions werden trotzdem gespiegelt)
- `--regen-docs` — nur `pp`: OneDrive-Mütze-Dokumente für bereits registrierte Kontakte (neu) erzeugen, **ohne** WhatsApp-Versand. Nützlich, wenn der OneDrive-Login beim ursprünglichen Lauf fehlschlug oder Dokumente nachgeneriert werden sollen.
- `--regen-rows "132,135"` — komma-separierte Zeilen-Indizes (1-basiert) für `--regen-docs`; leer = alle registrierten Kontakte

#### E-Mail-Fallback (nicht auf WhatsApp → Gmail)

Ist eine Anmeldung **nicht auf WhatsApp** erreichbar, wird die Willkommensnachricht automatisch **per E-Mail** verschickt — gleicher Text, mit dem Zürichsee-PNG im Anhang. Versand über die **Gmail API** von `zdavatz@gmail.com`. Die E-Mail-Adresse wird automatisch aus der Sheet-Kopfzeile erkannt (Spalte mit „mail" im Namen). Erfolgreich gemailte Empfänger werden — wie WhatsApp-Empfänger — als begrüsst markiert und nicht erneut angeschrieben.

Authentifizierung via **Application Default Credentials** mit dem Scope `gmail.send`. Einmalige Einrichtung (eigener OAuth-Desktop-Client nötig, da der Standard-gcloud-Client `gmail.send` nicht anfordern darf):

```bash
# Gmail API im Projekt aktivieren
gcloud services enable gmail.googleapis.com --project pegelstand
# OAuth-Consent-Screen: zdavatz@gmail.com als Testnutzer + Scope gmail.send eintragen,
# eigenen Desktop-OAuth-Client erstellen und JSON nach ~/gmail-oauth-client.json laden.
# Danach ADC mit gmail.send erzeugen (direkter Loopback-Consent, prompt=consent).
```

Die ADC liegt unter `~/.config/gcloud/application_default_credentials.json` (ausserhalb des Repos, nichts Geheimes wird committet). Fehlt sie oder fehlt der `gmail.send`-Scope, überspringt `welcome` den E-Mail-Fallback mit einem Hinweis; der WhatsApp-Versand läuft davon unberührt.

**OneDrive (nur `pp`):** Nach erfolgreichem WhatsApp-Versand wird pro Empfänger aus dem Word-Template `Vorname Nachname.docx` (in `/Dokumente/wakethief`) eine personalisierte Kopie erzeugt — Platzhalter `{{NAME}}/{{STRASSE}}/{{ORT}}` werden durch Sheet-Daten ersetzt (Adresse aus Spalte F) — und ins selbe Verzeichnis hochgeladen. Auth via Device-Code-Flow (`src/onedrive.rs`), Token gecacht in `whatsapp/onedrive-token.json` (gitignored). Die Azure-App-Registrierung muss **persönliche Microsoft-Konten** unterstützen (`signInAudience = AzureADandPersonalMicrosoftAccount`), Access-Token-Version 2 und "öffentliche Clientflows zulassen" = Ja. Die Template-Item-ID ist die OneDrive-Personal-Form (`8DB…!s…`), **nicht** die `resid`-GUID aus der Web-URL — letztere ist über Graph `/items/` nicht adressierbar (auflösbar via `/shares/`).

**Daten-Speicherung** in `whatsapp/contacts*.db` (SQLite, gitignored):
- `submissions` — vollständiger Formular-Snapshot, eine Zeile pro Antwort. Jede Sheet-Spalte wird zu einer eigenen TEXT-Spalte (Header sanitiert: kleingeschrieben, nicht-alnum → `_`, max. 50 Zeichen). Zusätzlich `data`-Spalte mit JSON-Blob (Source of Truth). Neue Headers fügen via `ALTER TABLE` Spalten hinzu und backfillen aus dem JSON.
- `contacts` — wer den Willkommensgruß erhalten hat (`jid` als PK → Re-Runs senden niemandem doppelt). Wird erst **nach** erfolgreichem WhatsApp-Send geschrieben — wer nicht auf WhatsApp ist, wird beim nächsten Lauf erneut versucht.

Beispiel-Abfragen:
```bash
sqlite3 whatsapp/contacts.db "SELECT vorname_first_name, mobile_whatsapp_if_possible FROM submissions LIMIT 5"
sqlite3 whatsapp/contacts_pp.db "SELECT name, surname, preferred_color_of_hat FROM submissions LIMIT 5"
sqlite3 whatsapp/contacts.db "SELECT COUNT(*) FROM contacts"   # wie viele bereits begrüsst
```

**Telefonnummern-Normalisierung** in `src/sync_contacts.rs`:
- Schweizer Formate (`079 822 93 58`, `0041…`) → E.164 (`+41…`)
- Mehrere `+`-Nummern in einer Zelle (`+41 … (Whatsapp: +48…)`) → bevorzugt die, die der Annotation "whatsapp" am nächsten steht
- Bare 9-stellige Schweizer Mobile (`779146476`) → `+41779146476` via Heuristik
- Längen-Sanity: 10–15 Ziffern; `+41`-Nummern müssen exakt 12 Zeichen lang sein
- Typos werden geloggt und übersprungen (nicht in DB)

**Einmalige Einrichtung — Google Sheets API** (Service Account, kein öffentliches Teilen nötig):

1. https://console.cloud.google.com → Projekt anlegen oder wählen
2. APIs & Services → Library → **Google Sheets API** → Enable
3. APIs & Services → Credentials → Create credentials → **Service account** → Name → Done
4. Service-Account anklicken → **Keys** → Add key → JSON → herunterladen
5. Datei nach `whatsapp/google-sa.json` verschieben (gitignored)
6. `client_email` aus der JSON kopieren → Sheet öffnen → **Share** → Email einfügen → **Viewer** → "Notify people" deaktivieren → Send

Beim ersten Aufruf ohne Key zeigt das CLI dieselben Schritte mit dem korrekten Zielpfad an.

`pegelstand welcome` (oder `sync-contacts`) ruft intern `whatsapp/check-and-send.mjs` auf — dieses Baileys-Helfer-Script verifiziert jede Nummer via `sock.onWhatsApp(jid)` und sendet nur an tatsächlich registrierte WhatsApp-Konten (mit 1.5s Pause pro Send als sanfte Rate-Limit-Bremse).

## Wichtige Stationen

| ID   | Name              | Gewässer              | Quelle     |
|------|-------------------|-----------------------|------------|
| 2209 | Zürich            | Zürichsee             | BAFU       |
| 2073 | Silvaplana        | Silvaplanersee        | BAFU       |
| 2154 | Grandson          | Lac de Neuchâtel      | BAFU       |
| 2025 | Brunnen           | Vierwaldstättersee    | BAFU       |
| 2082 | Greifensee        | Greifensee            | BAFU       |
| SIA  | Segl-Maria (Sils) | bei Silvaplanersee    | MeteoSwiss |
| PAY  | Payerne           | bei Neuenburgersee    | MeteoSwiss |
| ALT  | Altdorf           | bei Urnersee          | MeteoSwiss |
| PFA  | Pfaffikon ZH      | bei Greifensee        | MeteoSwiss |
| EIN  | Einsiedeln        | bei Sihlsee           | MeteoSwiss |
| —    | Ermioni           | Argolischer Golf      | Open-Meteo |
| —    | Saronikos-Boje    | ~30 km NE Ermioni     | Poseidon/HCMR |
| —    | Palea Fokea       | Saronischer Golf      | Poseidon/HCMR |

## Lizenz

- BAFU-Daten: [Liefer- und Nutzungsbedingungen des BAFU](https://www.bafu.admin.ch)
- Tecdottir: Open Data der Stadt Zürich
- Open-Meteo: [CC BY 4.0](https://open-meteo.com/en/terms) (Attribution erforderlich)
- Poseidon/HCMR: Registrierung unter https://auth.poseidon.hcmr.gr/auth/register/
