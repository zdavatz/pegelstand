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

### Zürichsee Wassertemperatur (Wasserschutzpolizei)

```bash
pegelstand seetemperatur --aktuell                     # Aktueller Wert
pegelstand seetemperatur                               # 3 Monate Tageswerte
pegelstand seetemperatur --datum 2026-04-08            # Alle 10-Min-Werte eines Tages
pegelstand seetemperatur -S mythenquai --aktuell       # Station Mythenquai
pegelstand seetemperatur --start 2025-06-01 --end 2025-09-01  # Eigener Zeitraum
```

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
