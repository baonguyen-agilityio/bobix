// Files behind a project's pet picture: the original the person chose (kept so it can be adjusted later) and the
// pixel-art result made from it. Port of the 0.1 (Electron) app's core/petstore.js; same folder, names and settings entries.
// <dir>/<file>            results, served to our pages by /pet-image/
// <dir>/originals/<src>   originals, in a sub folder the route cannot reach
use fancy_regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs, path::{Path, PathBuf}, sync::LazyLock};

pub const MAX_PICTURE: usize = 2 * 1024 * 1024; // bytes of a chosen file, and of a result

pub fn dir() -> PathBuf { crate::paths::data_dir().join("pets") }

fn hash(b: &[u8]) -> String { Sha256::digest(b).iter().take(12).map(|x| format!("{x:02x}")).collect() }

// What a picture really is, from its first bytes: the file name a person gave it proves nothing.
pub fn kind_of(b: &[u8]) -> Option<&'static str> {
    if b.len() >= 12 {
        if b[0] == 0x89 && &b[1..4] == b"PNG" { return Some("png"); }
        if b[..3] == [0xff, 0xd8, 0xff] { return Some("jpg"); }
        if &b[..4] == b"GIF8" { return Some("gif"); }
        if &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" { return Some("webp"); }
    }
    is_svg(b).then_some("svg")
}

// An SVG is never served or shown as it is: the panel draws it as an image (src/ui/pet-import.js), which runs no
// script and loads nothing outside the file, and only the pixel art made from it is stored as a result.
// Up to the root element: a BOM, an XML declaration, comments and a DOCTYPE without an internal subset. Two patterns,
// not one: joined, a comment could stretch over a non-SVG root up to a later "-->".
static PROLOG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\x{feff}?\s*(?:<\?xml[^>]*\?>\s*)?(?:(?:<!--[\s\S]*?-->|<!DOCTYPE[^>\[]*>)\s*)*").unwrap());
static ROOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^<svg(?=[\s>/])([^>]*)>").unwrap());
static ATTR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"([\w:-]+)\s*=\s*(?:"([^"]*)"|'([^']*)')"#).unwrap());
static LENGTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^\s*(\d*\.?\d+(?:e[-+]?\d+)?)\s*(px|pt|pc|in|cm|mm)?\s*$").unwrap());
const SVG_HEAD: usize = 64 * 1024; // the root tag must start within this many bytes
pub const SVG_SIDE: u32 = 1024; // the long side an SVG is drawn at, as the page's DECODE_SIDE

// The root <svg> tag's attributes, or None when the file's root element is something else.
fn svg_root(b: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(&b[..b.len().min(SVG_HEAD)]);
    let rest = &text[PROLOG.find(&text).ok()??.end()..];
    Some(ROOT.captures(rest).ok()??.get(1)?.as_str().to_string())
}

// Entity declarations are refused: they can make a small file expand into a huge one while it is parsed.
fn is_svg(b: &[u8]) -> bool { svg_root(b).is_some() && !b.windows(8).any(|w| w == b"<!ENTITY") }

// An absolute length in px; None for a percentage, a unit relative to text, zero or anything unreadable.
fn length_px(v: Option<&String>) -> Option<f64> {
    let c = LENGTH.captures(v?).ok()??;
    let unit = c.get(2).map_or("px", |m| m.as_str()).to_ascii_lowercase();
    let k = match unit.as_str() { "pt" => 4.0 / 3.0, "pc" => 16.0, "in" => 96.0, "cm" => 96.0 / 2.54, "mm" => 96.0 / 25.4, _ => 1.0 };
    let n = c[1].parse::<f64>().ok()? * k;
    (n > 0.0 && n.is_finite()).then_some(n)
}

// The size to draw an SVG at: its shape from the viewBox, else from an absolute width and height, else the 300×150 a
// browser gives an SVG with neither; the long side is always `side`, the shape kept within 8:1. Drawn at a fixed size,
// a small icon is not blurred and a huge declared size costs nothing.
pub fn draw_size(b: &[u8], side: u32) -> (u32, u32) {
    let root = svg_root(b).unwrap_or_default();
    let mut attrs = std::collections::HashMap::new();
    for c in ATTR.captures_iter(&root).flatten() {
        attrs.insert(c[1].to_ascii_lowercase(), c.get(2).or(c.get(3)).map_or("", |m| m.as_str()).to_string());
    }
    let vb: Vec<f64> = attrs.get("viewbox").map(|v| v.split(|c: char| c.is_whitespace() || c == ',').filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(f64::NAN)).collect()).unwrap_or_default();
    let (w, h) = if vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0 && vb.iter().all(|n| n.is_finite()) { (vb[2], vb[3]) } else {
        (length_px(attrs.get("width")).unwrap_or(300.0), length_px(attrs.get("height")).unwrap_or(150.0))
    };
    let (r, s) = ((w / h).clamp(1.0 / 8.0, 8.0), side as f64);
    if r >= 1.0 { (side, ((s / r).round() as u32).max(1)) } else { (((s * r).round() as u32).max(1), side) }
}

// A settings entry is a { src, file, opts } object, or a file name from before pictures were processed.
fn files_of(e: &Value) -> Vec<String> {
    match e {
        Value::String(s) => vec![s.clone()],
        Value::Object(o) => vec![format!("originals/{}", o.get("src").and_then(Value::as_str).unwrap_or("")), o.get("file").and_then(Value::as_str).unwrap_or("").into()],
        _ => vec![],
    }
}
pub fn served(e: &Value) -> Option<String> {
    match e { Value::String(s) => Some(s.clone()), Value::Object(o) => o.get("file").and_then(Value::as_str).map(String::from), _ => None }
}

// A name is the hash of its bytes: an existing file is the same file. So a write cut short (a full disk) must never
// leave a file under that name: written to a temporary file, then renamed.
fn write(dir: &Path, rel: &str, data: &[u8]) -> std::io::Result<()> {
    let f = dir.join(rel);
    fs::create_dir_all(f.parent().unwrap())?;
    if f.exists() { return Ok(()); }
    let tmp = f.with_extension(format!("{}.tmp", f.extension().and_then(|e| e.to_str()).unwrap_or("")));
    let r = fs::write(&tmp, data).and_then(|_| fs::rename(&tmp, &f));
    if r.is_err() { let _ = fs::remove_file(&tmp); }
    r
}

// Stores an import and returns the pictures map with this project's entry set. The caller saves the settings, then
// calls release() with the entry it replaced.
pub fn save_import(dir: &Path, images: &Value, project: &str, original: &[u8], kind: &str, result: &[u8], ext: &str, opts: &Value) -> std::io::Result<Value> {
    let (src, file) = (format!("{}.{kind}", hash(original)), format!("{}.{ext}", hash(result)));
    write(dir, &format!("originals/{src}"), original)?;
    write(dir, &file, result)?;
    let mut out = if images.is_object() { images.clone() } else { json!({}) };
    out[project] = json!({ "src": src, "file": file, "opts": opts });
    Ok(out)
}

// Deletes the files of `entry` (the one replaced or reset) that no entry of `images` uses any more: two projects can
// share a file, since a file is named after its bytes.
pub fn release(dir: &Path, images: &Value, entry: &Value) -> Vec<String> {
    let used: HashSet<String> = images.as_object().into_iter().flatten().flat_map(|(_, e)| files_of(e)).collect();
    let gone: Vec<String> = files_of(entry).into_iter().filter(|f| !used.contains(f)).collect();
    for f in &gone { let _ = fs::remove_file(dir.join(f)); }
    gone
}

// The stored original and the options it was made with, for adjusting it; None for a legacy entry or a lost file.
pub fn read_original(dir: &Path, entry: &Value) -> Option<(Vec<u8>, Value)> {
    let src = entry.as_object()?.get("src")?.as_str()?;
    Some((fs::read(dir.join("originals").join(src)).ok()?, entry["opts"].clone()))
}

// A result the panel made is what Save may store: a PNG or GIF of pixel-art size (at most 64 px a side), ≤ 2 MB.
pub fn result_ok(b: &[u8], ext: &str) -> bool {
    if b.len() > MAX_PICTURE || kind_of(b) != Some(ext) { return false; }
    let (w, h) = match ext {
        "png" if b.len() >= 24 && &b[12..16] == b"IHDR" => (u32::from_be_bytes(b[16..20].try_into().unwrap()), u32::from_be_bytes(b[20..24].try_into().unwrap())),
        "gif" => (u16::from_le_bytes([b[6], b[7]]) as u32, u16::from_le_bytes([b[8], b[9]]) as u32),
        _ => return false,
    };
    (1..=64).contains(&w) && (1..=64).contains(&h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(tail: &[u8]) -> Vec<u8> { [&[0x89, 0x50, 0x4e, 0x47, 13, 10, 26, 10][..], tail].concat() }
    fn tmp(n: &str) -> PathBuf { let d = std::env::temp_dir().join(format!("bobix-petstore-{n}-{}", std::process::id())); let _ = fs::remove_dir_all(&d); d }
    fn list(d: &Path) -> Vec<String> {
        let names = |p: &Path, pre: &str| -> Vec<String> { fs::read_dir(p).map(|r| r.flatten().map(|e| format!("{pre}{}", e.file_name().to_string_lossy())).filter(|n| n != "originals").collect()).unwrap_or_default() };
        let mut v = names(d, "");
        v.extend(names(&d.join("originals"), "originals/"));
        v.sort();
        v
    }

    #[test]
    fn kind_reads_the_bytes_not_the_name() {
        assert_eq!(kind_of(&png(b"xxxxxxxx")), Some("png"));
        assert_eq!(kind_of(&[0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0, 0, 0, 0, 0]), Some("jpg"));
        assert_eq!(kind_of(b"GIF89a......"), Some("gif"));
        assert_eq!(kind_of(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(kind_of(b"RIFF\0\0\0\0WAVEfmt "), None);
        assert_eq!(kind_of(b"<html><body><svg xmlns=\"http://www.w3.org/2000/svg\"/></body></html>"), None);
    }

    #[test]
    fn an_svg_is_known_by_its_root_tag() {
        let svg = |s: &str| kind_of(s.as_bytes());
        assert_eq!(svg("<svg/>"), Some("svg"), "shorter than any other kind's magic bytes");
        assert_eq!(svg("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 4 4\"><rect/></svg>"), Some("svg"));
        assert_eq!(svg("\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- made by hand -->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n<SVG\n width=\"4\">"), Some("svg"));
        assert_eq!(svg("<html><svg/></html>"), None, "an SVG inside HTML is HTML");
        assert_eq!(svg("<!-- <svg> --><html/>"), None, "a root tag inside a comment");
        assert_eq!(svg("<!-- a --><html><!-- b --><svg/>"), None, "a comment does not stretch over another root");
        assert_eq!(svg("<svgx/>"), None);
        assert_eq!(svg("<!DOCTYPE svg [<!ELEMENT svg ANY>]><svg/>"), None, "a DOCTYPE with an internal subset");
        assert_eq!(svg("<!DOCTYPE svg [ ]><svg/>"), None, "even an empty one");
        assert_eq!(svg("<svg><!ENTITY a \"aaaa\"></svg>"), None, "<!ENTITY anywhere in the file");
        assert_eq!(svg(&format!("{}<svg/>", " ".repeat(SVG_HEAD))), None, "the root tag starts too late");
    }

    #[test]
    fn an_svg_is_drawn_at_its_own_shape() {
        let size = |s: &str| draw_size(s.as_bytes(), 1024);
        assert_eq!(size("<svg viewBox=\"0 0 40 20\" width=\"10\" height=\"10\">"), (1024, 512), "the viewBox wins");
        assert_eq!(size("<svg viewBox='0,0,10,30'>"), (341, 1024));
        assert_eq!(size("<svg width=\"2in\" height=\"96px\">"), (1024, 512), "absolute units");
        assert_eq!(size("<svg viewBox=\"0 0 0 10\" width=\"30\" height=\"10\">"), (1024, 341), "an empty viewBox falls back");
        assert_eq!(size("<svg>"), (1024, 512), "300×150 without a size");
        assert_eq!(size("<svg width=\"50%\" height=\"inf\">"), (1024, 512), "relative or unreadable lengths count as missing");
        assert_eq!(size("<svg width=\"300\">"), (1024, 512), "a missing height alone is 150");
        assert_eq!(size("<svg viewBox=\"0 0 1000 1\">"), (1024, 128), "never wider than 8:1");
        assert_eq!(size("<svg viewBox=\"0 0 1 1000\">"), (128, 1024), "never taller than 1:8");
        assert_eq!(draw_size(b"<svg viewBox=\"0 0 4000 1\">", 4), (4, 1), "at least 1 px");
    }

    #[test]
    fn an_import_is_stored_and_read_back() {
        let d = tmp("a");
        let opts = json!({ "size": 32 });
        let images = save_import(&d, &json!({}), "/p", &png(b"the original"), "png", b"GIF89a result", "gif", &opts).unwrap();
        let e = &images["/p"];
        assert!(e["src"].as_str().unwrap().len() == 28 && e["src"].as_str().unwrap().ends_with(".png"));
        assert!(e["file"].as_str().unwrap().ends_with(".gif"));
        assert_eq!(fs::read(d.join(e["file"].as_str().unwrap())).unwrap(), b"GIF89a result");
        assert_eq!(served(e), e["file"].as_str().map(String::from));
        assert_eq!(read_original(&d, e), Some((png(b"the original"), opts)));
        let entry = crate::settings::sanitize(&json!({ "petImages": images }), &crate::settings::defaults())["petImages"]["/p"].clone();
        assert_eq!(entry["src"], e["src"], "what is stored survives the settings check");
        fs::remove_file(d.join("originals").join(e["src"].as_str().unwrap())).unwrap();
        assert_eq!(read_original(&d, e), None, "a lost original");
        assert_eq!(read_original(&d, &e["file"]), None, "a legacy entry has no original");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn an_svg_original_is_kept_for_edit() {
        let d = tmp("s");
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 2 1\"/>";
        let images = save_import(&d, &json!({}), "/s", svg, "svg", b"GIF89a result", "gif", &json!({})).unwrap();
        let entry = crate::settings::sanitize(&json!({ "petImages": images }), &crate::settings::defaults())["petImages"]["/s"].clone();
        assert!(entry["src"].as_str().is_some_and(|s| s.ends_with(".svg")), "kept by the settings check: {entry}");
        let (original, _) = read_original(&d, &entry).unwrap();
        assert_eq!((original.as_slice(), kind_of(&original)), (&svg[..], Some("svg")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn release_deletes_only_unused_files() {
        let d = tmp("r");
        let o = json!({});
        let mut images = save_import(&d, &json!({}), "/a", &png(b"one"), "png", b"shared result", "png", &o).unwrap();
        images = save_import(&d, &images, "/b", &png(b"one"), "png", b"shared result", "png", &o).unwrap();
        assert_eq!(images["/a"]["file"], images["/b"]["file"]);
        assert_eq!(list(&d).len(), 2);
        let old = images["/a"].clone();
        images = save_import(&d, &images, "/a", &png(b"two"), "png", b"second result", "png", &o).unwrap();
        assert!(release(&d, &images, &old).is_empty(), "a file /b still uses stays");
        assert_eq!(list(&d).len(), 4);
        let old_b = images["/b"].clone();
        let mut rest = images.clone();
        rest.as_object_mut().unwrap().shift_remove("/b");
        assert_eq!(release(&d, &rest, &old_b).len(), 2);
        let mut want = vec![images["/a"]["file"].as_str().unwrap().to_string(), format!("originals/{}", images["/a"]["src"].as_str().unwrap())];
        want.sort();
        assert_eq!(list(&d), want);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn save_takes_only_pixel_art() {
        let mut p = png(b"\0\0\0\rIHDR");
        p.extend_from_slice(&48u32.to_be_bytes());
        p.extend_from_slice(&40u32.to_be_bytes());
        assert!(result_ok(&p, "png"));
        assert!(!result_ok(&p, "gif"));
        let mut big = png(b"\0\0\0\rIHDR");
        big.extend_from_slice(&1024u32.to_be_bytes());
        big.extend_from_slice(&1024u32.to_be_bytes());
        assert!(!result_ok(&big, "png"), "not pixel art");
        assert!(result_ok(b"GIF89a\x20\x00\x20\x00....", "gif"));
        assert!(!result_ok(&[0xff, 0xd8, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0], "png"));
    }
}
