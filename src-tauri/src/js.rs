// The few JavaScript semantics the ported code depends on, so Rust gives the same results as src/core/*.js.
use serde_json::Value;

// Number(v)
pub fn js_number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::Bool(b) => *b as u8 as f64,
        Value::Null => 0.0,
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() { return 0.0; }
            if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                return i64::from_str_radix(h, 16).map(|n| n as f64).unwrap_or(f64::NAN);
            }
            match t {
                "Infinity" | "+Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ if t.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)) => t.parse().unwrap_or(f64::NAN),
                _ => f64::NAN,
            }
        }
        _ => f64::NAN,
    }
}

// Math.round: halves go up.
pub fn round(n: f64) -> f64 { (n + 0.5).floor() }

// String length as JavaScript counts it (UTF-16 code units).
pub fn len16(s: &str) -> usize { s.encode_utf16().count() }

// s.slice(0, n) in UTF-16 units, never splitting a character.
pub fn head16(s: &str, n: usize) -> &str {
    let mut units = 0;
    for (i, c) in s.char_indices() {
        units += c.len_utf16();
        if units > n { return &s[..i]; }
    }
    s
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
