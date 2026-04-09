# Pegelstand

CLI-Tool zur Abfrage von Schweizer Gewässerdaten (Pegel, Temperatur, Abfluss) via BAFU und Stadt Zürich APIs.

Pegelstand Infos zum Pumpfoilen.

## Datenquellen

- **BAFU / api.existenz.ch** — Pegelstände, Abfluss, Fluss-Temperaturen (237+ Stationen)
- **InfluxDB (api.existenz.ch)** — Historische Daten ab 2001
- **Wasserschutzpolizei Zürich (tecdottir)** — Zürichsee Wassertemperatur, Wetter (seit 2007)

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

### HTML-Report generieren

```bash
# Interaktiver Report (Chart.js, für Browser)
pegelstand report --start 2026-03-25 --end 2026-03-26

# SVG-Report (kein JavaScript, für WhatsApp/Mail/Offline)
pegelstand report --start 2026-03-25 --end 2026-03-26 --svg

# Eigene Ausgabedatei
pegelstand report --start 2026-03-25 --end 2026-03-26 --svg -o bericht.html
```

Der Report enthält:
- Statistik-Karten (Min/Max Wassertemp, Windchill, Böen, Beaufort, Luftdruck)
- Charts: Temperaturverlauf, Wind/Böen, Windrichtung, Luftdruck, Pegel
- Vollständige Datentabelle (alle 10-Minuten-Messwerte)
- Quellenangabe pro Feld (T = Tiefenbrunnen, M = Mythenquai)

| Modus | Dateigrösse | JavaScript | WhatsApp | Interaktiv |
|-------|-------------|------------|----------|------------|
| Chart.js | ~244 KB | ja | nein | ja (Hover) |
| `--svg` | ~124 KB | nein | ja | nein |

## Wichtige Stationen

| ID   | Name              | Gewässer    |
|------|-------------------|-------------|
| 2209 | Zürich            | Zürichsee   |
| 2014 | Schmerikon        | Zürichsee   |
| 2099 | Zürich Unterhard  | Limmat      |
| 2176 | Zürich            | Sihl        |
| 2135 | Bern, Schönau     | Aare        |

## Lizenz

BAFU-Daten unterliegen den [Liefer- und Nutzungsbedingungen des BAFU](https://www.bafu.admin.ch). Tecdottir-Daten sind Open Data der Stadt Zürich.
