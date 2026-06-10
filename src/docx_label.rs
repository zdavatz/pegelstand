// Minimal .docx placeholder replacer.
//
// A .docx is a ZIP archive; `word/document.xml` holds the body. Word can
// split a typed string across multiple `<w:r>` runs (e.g. after an autocorrect
// or a font change mid-word). We therefore do a two-step replace:
//
//   1. Try a direct substring replace inside each `<w:t>` text element. This
//      covers the common case where the placeholder was typed in one go.
//   2. If a placeholder survives step 1, do a run-merge: within a single
//      `<w:p>` paragraph, concatenate consecutive `<w:t>` contents into a
//      virtual string, find the placeholder, and rewrite the paragraph with
//      the value substituted.
//
// Values may contain `\n` for line breaks → emitted as `<w:br/>`.

use std::io::{Cursor, Read, Write};

pub fn replace_placeholders(
    docx: &[u8],
    repls: &[(&str, &str)],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let reader = Cursor::new(docx);
    let mut zip = zip::ZipArchive::new(reader)?;

    let mut out_buf = Vec::new();
    {
        let cursor = Cursor::new(&mut out_buf);
        let mut writer = zip::ZipWriter::new(cursor);

        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
                .compression_method(entry.compression())
                .last_modified_time(entry.last_modified().unwrap_or_default());

            if name == "word/document.xml" {
                let mut xml = String::new();
                entry.read_to_string(&mut xml)?;
                let new_xml = apply_replacements(&xml, repls);
                writer.start_file(name, options)?;
                writer.write_all(new_xml.as_bytes())?;
            } else {
                writer.start_file(name, options)?;
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                writer.write_all(&data)?;
            }
        }
        writer.finish()?;
    }
    Ok(out_buf)
}

fn apply_replacements(xml: &str, repls: &[(&str, &str)]) -> String {
    let mut s = xml.to_string();

    // Step 1: literal replace inside `<w:t>...</w:t>` text-element bodies.
    for (ph, val) in repls {
        s = replace_in_text_elements(&s, ph, val);
    }

    // Step 2: if any placeholder still survives, fall back to paragraph-level
    // merge-and-replace. Slow path; rare unless the template was reformatted.
    if repls.iter().any(|(ph, _)| s.contains(ph)) {
        s = replace_across_runs(&s, repls);
    }

    s
}

/// Find each `<w:t ...>BODY</w:t>` and replace `ph` → `val` inside BODY.
/// If `val` contains `\n`, emit `<w:br/>` between segments (this requires
/// closing the current `<w:t>`, inserting `</w:r>` + run with `<w:br/>` +
/// new run, then re-opening `<w:t>` — handled by `inject_value`).
fn replace_in_text_elements(xml: &str, ph: &str, val: &str) -> String {
    // Find `<w:t` ... `>` ... `</w:t>` blocks. The opening tag may carry
    // attributes (e.g. `xml:space="preserve"`), so we match opening up to '>'.
    let mut out = String::with_capacity(xml.len() + 64);
    let mut rest = xml;
    while let Some(open_start) = rest.find("<w:t") {
        out.push_str(&rest[..open_start]);
        let after_open = &rest[open_start..];
        let Some(tag_end) = after_open.find('>') else {
            out.push_str(after_open);
            return out;
        };
        let open_tag = &after_open[..=tag_end]; // "<w:t ...>"
        // Skip self-closing or empty tags.
        if open_tag.ends_with("/>") {
            out.push_str(open_tag);
            rest = &after_open[tag_end + 1..];
            continue;
        }
        let body_start = tag_end + 1;
        let Some(close_rel) = after_open[body_start..].find("</w:t>") else {
            out.push_str(after_open);
            return out;
        };
        let body = &after_open[body_start..body_start + close_rel];
        let body_end = body_start + close_rel + "</w:t>".len();

        out.push_str(open_tag);
        if body.contains(ph) {
            let replaced = body.replace(ph, val);
            if replaced.contains('\n') {
                // Need to close <w:t>, then for each segment emit a separate
                // run/<w:t>. We assume the open_tag already declared
                // xml:space="preserve" or doesn't strip; we'll add preserve
                // to our emitted opens defensively.
                let segs: Vec<&str> = replaced.split('\n').collect();
                out.push_str(&xml_escape(segs[0]));
                out.push_str("</w:t>");
                for seg in &segs[1..] {
                    out.push_str("<w:br/>");
                    out.push_str("<w:t xml:space=\"preserve\">");
                    out.push_str(&xml_escape(seg));
                }
                // The next `</w:r>` after the original block remains; the
                // last `<w:t>` we just emitted needs its own closer. We'll
                // emit it here, then SKIP the original closer below.
                out.push_str("</w:t>");
                rest = &after_open[body_end..];
                continue;
            } else {
                out.push_str(&xml_escape(&replaced));
            }
        } else {
            out.push_str(body);
        }
        out.push_str("</w:t>");
        rest = &after_open[body_end..];
    }
    out.push_str(rest);
    out
}

/// Slow path: within each `<w:p>...</w:p>` paragraph, concatenate `<w:t>`
/// contents and try to replace placeholders that span runs. If matched,
/// rewrite the entire paragraph using a single simple run that carries the
/// substituted text (loses inline formatting between the run boundaries that
/// straddled the placeholder — acceptable trade-off).
fn replace_across_runs(xml: &str, repls: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(p_start) = rest.find("<w:p ").or_else(|| rest.find("<w:p>")) {
        out.push_str(&rest[..p_start]);
        let after = &rest[p_start..];
        let Some(p_end_rel) = after.find("</w:p>") else {
            out.push_str(after);
            return out;
        };
        let p_full = &after[..p_end_rel + "</w:p>".len()];

        // Concatenate all <w:t>BODY</w:t> contents in the paragraph.
        let concat = extract_text(p_full);
        let mut needs_rewrite = false;
        let mut new_text = concat.clone();
        for (ph, val) in repls {
            if new_text.contains(ph) {
                new_text = new_text.replace(ph, val);
                needs_rewrite = true;
            }
        }
        if needs_rewrite {
            // Locate the original paragraph properties (<w:pPr>...</w:pPr>) and
            // first <w:rPr> to preserve formatting cues.
            let ppr = extract_block(p_full, "<w:pPr>", "</w:pPr>").unwrap_or_default();
            let rpr = extract_block(p_full, "<w:rPr>", "</w:rPr>").unwrap_or_default();
            out.push_str("<w:p>");
            out.push_str(&ppr);
            out.push_str("<w:r>");
            out.push_str(&rpr);
            for (i, seg) in new_text.split('\n').enumerate() {
                if i > 0 { out.push_str("<w:br/>"); }
                out.push_str("<w:t xml:space=\"preserve\">");
                out.push_str(&xml_escape(seg));
                out.push_str("</w:t>");
            }
            out.push_str("</w:r></w:p>");
        } else {
            out.push_str(p_full);
        }
        rest = &after[p_end_rel + "</w:p>".len()..];
    }
    out.push_str(rest);
    out
}

fn extract_text(p_xml: &str) -> String {
    let mut out = String::new();
    let mut rest = p_xml;
    while let Some(t_start) = rest.find("<w:t") {
        let after = &rest[t_start..];
        let Some(tag_end) = after.find('>') else { break; };
        let open = &after[..=tag_end];
        if open.ends_with("/>") {
            rest = &after[tag_end + 1..];
            continue;
        }
        let body_start = tag_end + 1;
        let Some(close_rel) = after[body_start..].find("</w:t>") else { break; };
        out.push_str(&xml_unescape(&after[body_start..body_start + close_rel]));
        rest = &after[body_start + close_rel + "</w:t>".len()..];
    }
    out
}

fn extract_block(xml: &str, start: &str, end: &str) -> Option<String> {
    let s = xml.find(start)?;
    let e = xml[s..].find(end)? + s + end.len();
    Some(xml[s..e].to_string())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}
