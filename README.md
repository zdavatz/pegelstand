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
pegelstand ermioni                                     # Letzte 7 Tage (stündlich)
pegelstand ermioni --start 2025-07-01 --end 2025-07-31 # Eigener Zeitraum
```

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

Reine SVG-Datei mit Temperatur, Pegelstand, Wind/Böen und Luftdruck — kein HTML, kein JavaScript. Ideal für Einbettung, WhatsApp, E-Mail.

```bash
pegelstand svg                                             # Letzte 5 Tage (Standard)
pegelstand svg --start 2026-04-01 --end 2026-04-10         # Eigener Zeitraum
pegelstand svg -o mein_chart.svg                           # Eigene Ausgabedatei
pegelstand svg --start 2026-04-10 --end 2026-04-11 --png   # SVG + PNG (für WhatsApp)
pegelstand svg --start 2026-04-10 --end 2026-04-11 --png --whatsapp "GROUP_JID@g.us"  # PNG an WhatsApp-Gruppe senden
```

Ausgabe: SVG im `svg/`-Verzeichnis, PNG im `png/`-Verzeichnis. PNG wird mit 2x Auflösung (Retina) via `resvg` gerendert. Datumsformat: dd.mm.yyyy. Quellen: Tiefenbrunnen (T) + Mythenquai (M).

4 Charts: Temperatur (Wasser + Luft), Pegelstand, Wind & Böen, Luftdruck.

#### WhatsApp-Integration

Das PNG kann direkt an eine WhatsApp-Gruppe gesendet werden via [Baileys](https://github.com/WhiskeySockets/Baileys) (WhatsApp Web Protokoll, Node.js).

```bash
pegelstand whatsapp login                                    # 1x QR-Code scannen
pegelstand whatsapp groups                                   # Gruppen-JIDs anzeigen
pegelstand whatsapp leave "GROUP_JID@g.us"                   # Gruppe verlassen
pegelstand svg --png --whatsapp "GROUP_JID@g.us"             # Generieren + senden
```

Beim ersten `login` wird ein QR-Code im Terminal angezeigt — mit WhatsApp scannen (Einstellungen → Verknüpfte Geräte). Die Session wird in `whatsapp/auth/` gespeichert. npm-Abhängigkeiten werden automatisch installiert.

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
