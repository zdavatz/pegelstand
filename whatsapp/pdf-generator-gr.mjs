#!/usr/bin/env node
// Greek translation of the Baugeschichte document. Same layout as
// pdf-generator.mjs, but all static template strings AND Erica's notes/captions
// are rendered in Greek via a translation map (TR), keyed by the trimmed German
// original. Unmapped text falls through unchanged so nothing is silently lost.
//
// Usage: node pdf-generator-gr.mjs <dir> <outHtml>

import { readFileSync, writeFileSync, existsSync } from "fs";
import { resolve, extname } from "path";

const DIR = resolve(process.argv[2] || "/tmp/erica_wa");
const OUT = resolve(process.argv[3] || "/tmp/erica_wa/baugeschichte_gr.html");
const data = JSON.parse(readFileSync(resolve(DIR, "messages.json"), "utf8"));
const m = data.matches[0] || { messages: [], name: "Erica Baumann" };

const esc = (s) => String(s || "").replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" }[c]));
const IMG = new Set([".jpg", ".jpeg", ".png", ".webp", ".gif"]);

// --- German original (trimmed) -> Greek ---
const TR = new Map([
  ["Guten Tag lieber Zeno\nGerne treffe ich dich zu einem Gespräch, das der Verkauf von meinem Häuschen in Ermioni betrifft. \nBitte mach mir einen Vorschlag. \n           Grüsse aus Höngg\n           Erika",
   "Καλημέρα αγαπητέ Ζένο\nΜε χαρά θα σε συναντήσω για μια συζήτηση που αφορά την πώληση του σπιτιού μου στην Ερμιόνη. \nΣε παρακαλώ κάνε μου μια πρόταση. \n           Χαιρετίσματα από το Höngg\n           Έρικα"],
  ["Kommt noch einiges mehr. Auch von der Gasse und Wohnzimmer vor 5-6 Jahren.",
   "Θα ακολουθήσουν κι άλλα. Επίσης από το σοκάκι και το σαλόνι πριν από 5-6 χρόνια."],
  ["Muss nun einen Unterbruch machen.",
   "Πρέπει τώρα να κάνω ένα διάλειμμα."],
  ["Entschuldige, habe beim durchlesen einige Flüchtigkeitsfehler entdeckt. \n            Am Nachmittag geht es weiter.",
   "Συγγνώμη, διαβάζοντας ξανά εντόπισα μερικά λαθάκια απροσεξίας. \n            Το απόγευμα συνεχίζουμε."],
  ["Zu meinem anderen Nachbar mit kl. Haus. Hauseingang zur Strasse. \nDas Haus hat er geerbt.\nAls ehemaliger Schiffsingenieur war sein Wunsch selbstständig einige Reparaturen am Haus zu machen. \nWir konnten uns gut verständigen trotz unterschiedlicher Sprache.",
   "Σχετικά με τον άλλο μου γείτονα με το μικρό σπίτι. Η είσοδος του σπιτιού προς τον δρόμο. \nΤο σπίτι το κληρονόμησε.\nΩς πρώην μηχανικός πλοίων, επιθυμία του ήταν να κάνει μόνος του κάποιες επισκευές στο σπίτι. \nΜπορούσαμε να συνεννοηθούμε καλά παρά τη διαφορετική γλώσσα."],
  ["Anfangs Jahr 1991 begleitete mich Hans nach Ermioni mit Material für Vermessungen vorzunehmen und einen Plan zuerstellen für die Arbeiter. Ich fotografierte drauf los. Notierte alle Veränderungen, die gemacht werden mussten. Die erste Material Einkäufe, die ich von der Schweiz transportieren wollte.",
   "Στις αρχές του 1991 με συνόδευσε ο Χανς στην Ερμιόνη με υλικά, για να κάνουμε μετρήσεις και να ετοιμάσουμε ένα σχέδιο για τους εργάτες. Φωτογράφιζα ασταμάτητα. Σημείωνα όλες τις αλλαγές που έπρεπε να γίνουν. Τις πρώτες αγορές υλικών που ήθελα να μεταφέρω από την Ελβετία."],
  ["1990 H.Besichtigung und gekauft.\nRe. Rotestor H.Eingang.\nWäsche der Nachbarn.",
   "1990 Επίσκεψη και αγορά του σπιτιού.\nΔεξιά κόκκινη πύλη, είσοδος του σπιτιού.\nΜπουγάδα των γειτόνων."],
  ["Abbruch von 2 kleinen Balkonen für Chminéeholz.\nLi. S. hinten Dusche m. WC. Re. Eingangstor.\nHeute geschlossener Duschraum.",
   "Κατεδάφιση 2 μικρών μπαλκονιών για ξύλα του τζακιού.\nΑριστερά πίσω ντους με WC. Δεξιά η πύλη εισόδου.\nΣήμερα κλειστός χώρος ντους."],
  ["Neuer TerrassenAufbau.\nNeuer doppelter Dach-\nIsolierung.",
   "Νέα κατασκευή της ταράτσας.\nΝέα διπλή μόνωση\nοροφής."],
  ["Anfertigung von zwei BalkonSeitenwände zu den Nachbarn.",
   "Κατασκευή δύο πλαϊνών τοίχων μπαλκονιού προς τους γείτονες."],
  ["Alte Küche mit einem kleinen Fenster.",
   "Παλιά κουζίνα με ένα μικρό παράθυρο."],
  ["Hinter der Küche und Duschraum kam eine kurze Treppe vom Nachbarhaus in m. Haus.\nAusgeräumt, umfunktioniert zu einem geschlossenen Putzschrank.",
   "Πίσω από την κουζίνα και τον χώρο ντους υπήρχε μια κοντή σκάλα από το γειτονικό σπίτι προς το δικό μου.\nΑδειάστηκε και μετατράπηκε σε κλειστή ντουλάπα καθαριστικών."],
  ["Durch einen kleinen Durchgang mit 2 grossen Bollensteine gelangte man\nIn Chminéeraum= Wohnraum.",
   "Μέσα από ένα μικρό πέρασμα με 2 μεγάλες πέτρες έφτανε κανείς\nστον χώρο του τζακιού = σαλόνι."],
  ["Fenster zur zur hinteren Gasse.",
   "Παράθυρο προς το πίσω σοκάκι."],
  ["Fenster im Wohnraum, Chminée, Sicht in einem Schacht zum Nachbar. Der mit grossen unvertigen grossen Haus.\nAnstelle vom Schacht wurde ein Kasten gebaut für Büroarbeiten.",
   "Παράθυρο στο σαλόνι, τζάκι, θέα μέσα από ένα φρεάτιο προς τον γείτονα — αυτόν με το μεγάλο ημιτελές σπίτι.\nΣτη θέση του φρεατίου κατασκευάστηκε ένα ερμάριο για γραφειακή εργασία."],
  ["Aufstieg in 1. Stock.",
   "Άνοδος στον 1ο όροφο."],
  ["Oberhalb der Treppe befindet sich ein AtrappenFenster. Auf passen beim Aufstieg.\n„Dunkelheit“. Böden wurden im ganzen erneuert.",
   "Πάνω από τη σκάλα υπάρχει ένα ψεύτικο παράθυρο. Προσοχή κατά την άνοδο.\n«Σκοτάδι». Τα δάπεδα ανανεώθηκαν εξ ολοκλήρου."],
  ["Fenster vom Wohnraum zur hinteren Strasse.",
   "Παράθυρο του σαλονιού προς τον πίσω δρόμο."],
  ["Ende Jahr 1990 war es soweit.",
   "Στα τέλη του 1990 ήρθε η στιγμή."],
  ["Jürg organisierte mir einen Übersetzer der mich zur Anwältin  mitnahm.",
   "Ο Γιούργκ μου κανόνισε έναν μεταφραστή που με πήγε στη δικηγόρο."],
  ["Beim Unterschreiben.  Somit hatte ich einen HausGötti.",
   "Κατά την υπογραφή. Έτσι απέκτησα έναν «νονό του σπιτιού»."],
  ["Beim Unterscheiben. Das Haus ist gekauft.",
   "Κατά την υπογραφή. Το σπίτι αγοράστηκε."],
  ["Hans erster Arbeitstag.",
   "Η πρώτη εργάσιμη μέρα του Χανς."],
  ["Hans beim Vermessen",
   "Ο Χανς κατά τη μέτρηση."],
  ["Hans macht erste Bekanntschaft mit Matina Notara.",
   "Ο Χανς γνωρίζεται για πρώτη φορά με τη Ματίνα Νοταρά."],
]);
const tr = (s) => { const k = String(s || "").trim(); return TR.has(k) ? TR.get(k) : s; };

const hers = m.messages.filter((x) => !x.fromMe).sort((a, b) => a.ts - b.ts);
const photos = hers.filter((x) => x.file && IMG.has(extname(x.file).toLowerCase()));
const textNotes = hers.filter((x) => !x.file && x.text && x.text.trim());

const intro = tr(textNotes[0]?.text || "");
const closing = textNotes.slice(1).map((x) => tr(x.text.trim()));

const figures = photos
  .map((p, i) => {
    const cap = tr((p.text || "").trim());
    return `<figure class="plate">
  <div class="num">Εικόνα ${i + 1}</div>
  <img src="${esc(p.file)}"/>
  ${cap ? `<figcaption>${esc(cap).replace(/\n/g, "<br/>")}</figcaption>` : ""}
</figure>`;
  })
  .join("\n");

const closingHtml = closing.length
  ? `<div class="note"><p>${closing.map(esc).map((t) => t.replace(/\n/g, "<br/>")).join("</p><p>")}</p>
     <p class="sig">— ${esc(m.name || "Erica Baumann")}</p></div>`
  : "";

const html = `<!doctype html><html lang="el"><head><meta charset="utf-8"><style>
@page { size: A4; margin: 20mm 18mm 18mm; }
@page :first { margin: 0; }
* { box-sizing: border-box; }
body { font-family: "Georgia", "DejaVu Serif", serif; color: #1f1a16; line-height: 1.5; }

/* Cover */
.cover { height: 297mm; width: 210mm; padding: 40mm 28mm; display: flex; flex-direction: column;
  justify-content: center; background: #2f2a25; color: #f5efe6; page-break-after: always; }
.cover .kicker { letter-spacing: 4px; text-transform: uppercase; font-size: 11pt; color: #cbb894; margin-bottom: 8mm; }
.cover h1 { font-size: 34pt; line-height: 1.15; margin: 0 0 6mm; font-weight: 700; }
.cover .place { font-size: 15pt; font-style: italic; }
.cover .place a { color: #e3c98e; text-decoration: underline; }
.cover .rule { width: 40mm; height: 2px; background: #cbb894; margin: 12mm 0; }
.cover .lead { font-size: 12pt; color: #e7ded2; max-width: 120mm; }

/* Intro */
.intro { margin: 0 0 12mm; }
.intro h2 { font-size: 15pt; color: #6b5a44; border-bottom: 1px solid #d9cdbb; padding-bottom: 2mm; margin: 0 0 5mm; }
.intro blockquote { margin: 0; padding: 4mm 6mm; border-left: 3px solid #cbb894; background: #faf6ef;
  font-style: italic; white-space: pre-wrap; font-size: 11.5pt; }

/* Photo plates */
.plate { margin: 0 0 12mm; page-break-inside: avoid; text-align: center; }
.plate .num { font-size: 9.5pt; letter-spacing: 3px; text-transform: uppercase; color: #a08b6a; margin-bottom: 3mm; }
.plate img { max-width: 100%; max-height: 165mm; border: 1px solid #ddd; box-shadow: 0 1mm 4mm rgba(0,0,0,.18); }
.plate figcaption { margin: 4mm auto 0; max-width: 100%; font-size: 11.5pt; color: #2f2a25;
  background: #f3ede3; padding: 3mm 6mm; border-radius: 1mm; text-align: left; }

/* Closing note */
.note { margin-top: 10mm; padding: 5mm 6mm; background: #faf6ef; border: 1px solid #e4d9c6; border-radius: 1mm;
  font-size: 11pt; color: #4a4035; }
.note .sig { text-align: right; font-style: italic; color: #6b5a44; margin-top: 4mm; }
.footer { margin-top: 14mm; text-align: center; font-size: 8.5pt; color: #9a8e7d; }
</style></head><body>

<section class="cover">
  <div class="kicker">Η ιστορία της κατασκευής σε εικόνες</div>
  <h1>Το σπίτι της<br/>${esc(m.name || "Erica Baumann")}</h1>
  <div class="place"><a href="https://www.google.com/maps/search/?api=1&amp;query=Ermioni%2C+Griechenland">Ερμιόνη, Ελλάδα</a></div>
  <div class="rule"></div>
  <div class="lead">Από την αγορά το 1990 έως τις μετέπειτα μετατροπές — μια φωτογραφική τεκμηρίωση των αλλαγών στο πέρασμα των χρόνων.</div>
</section>

${intro ? `<section class="intro">
  <h2>Η ιστορία του σπιτιού</h2>
  <blockquote>${esc(intro)}</blockquote>
</section>` : ""}

${figures}

${closingHtml}

<div class="footer">Συντάχθηκε από τις εικόνες και τις περιγραφές της ${esc(m.name || "Erica Baumann")} · Σπίτι στην Ερμιόνη</div>
</body></html>`;

writeFileSync(OUT, html);
const untranslated = [...textNotes.slice(1).map((x) => x.text.trim()), ...photos.map((p) => (p.text || "").trim()), (textNotes[0]?.text || "").trim()]
  .filter((s) => s && !TR.has(s));
console.log(`HTML written: ${OUT} — ${photos.length} Bilder, intro=${intro ? "yes" : "no"}, closing=${closing.length}`);
if (untranslated.length) console.log(`UNTRANSLATED (${untranslated.length}):\n` + untranslated.map((s) => "  · " + JSON.stringify(s.slice(0, 60))).join("\n"));
