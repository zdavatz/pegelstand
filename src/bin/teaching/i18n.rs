// All translatable strings of the teaching guide. The document structure,
// URL list and link order are fixed in main.rs — a language only supplies
// its texts. `version_line` contains `{}` for the build date.

pub struct T {
    pub code: &'static str,       // "EN", "DE", ... (CLI filter + log)
    pub meta_title: &'static str, // ASCII-only (PDF metadata is Latin-1)
    pub out_file: &'static str,
    pub cjk: bool,                // Arial Unicode + pre-wrapping instead of DejaVu
    pub qopen: &'static str,      // quotation marks used by quote()
    pub qclose: &'static str,

    pub title: &'static str,
    pub tagline: &'static str,
    pub byline: &'static str,
    pub version_line: &'static str,

    pub s1_h: &'static str,
    pub s1_q: &'static str,
    pub s1_att: &'static str,
    pub s1_b: &'static str,

    pub s2_h: &'static str,
    pub s2_b1: &'static str,
    pub s2_q1: &'static str,
    pub s2_a1: &'static str,
    pub s2_b2: &'static str,
    pub s2_q2: &'static str,
    pub s2_a2: &'static str,
    pub s2_b3: &'static str,

    pub s3_h: &'static str,
    pub s3_b: &'static str,

    pub s4_h: &'static str,
    pub s4_b: &'static str,

    pub s5_h: &'static str,
    pub s5_b: &'static str,

    pub s6_h: &'static str,
    pub s6_b: &'static str,
    pub s6_onix: &'static str,

    pub s7_h: &'static str,
    pub s7_intro: &'static str,
    pub sl_bkf: &'static str,
    pub sl_kneestart: &'static str,
    pub sl_boardtales: &'static str,
    pub sl_uponfoil: &'static str,
    pub sl_totalsup: &'static str,
    pub sl_schulung: &'static str,
    pub sl_pool: &'static str,
    pub sl_adhd: &'static str,
    pub sl_therapy: &'static str,
    pub sl_ig: &'static str,

    pub closing: &'static str,
}

pub fn all() -> Vec<T> {
    vec![
        crate::texts_en::t(),
        crate::texts_de::t(),
        crate::texts_fr::t(),
        crate::texts_it::t(),
        crate::texts_es::t(),
        crate::texts_zh::t(),
        crate::texts_ja::t(),
        crate::texts_ko::t(),
        crate::texts_el::t(),
    ]
}
