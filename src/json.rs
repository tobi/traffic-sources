//! A small, dependency-free JSON value type: parsing, serialisation and lookup.
//!
//! This is what the crate uses to read its rule files and NDJSON requests and to write its
//! results. It is public so callers can consume [`Attribution`](crate::Attribution) output or
//! build [`Request`](crate::Request)s without pulling in a JSON library of their own.
//!
//! ```
//! use traffic_sources::json::Value;
//!
//! let v = Value::parse(r#"{"url": "https://a.example/", "tags": [1, true, null]}"#).unwrap();
//! assert_eq!(v["url"].as_str(), Some("https://a.example/"));
//! assert_eq!(v["tags"][1].as_bool(), Some(true));
//! assert!(v["missing"].is_null());
//! assert_eq!(v.to_string(), r#"{"url":"https://a.example/","tags":[1,true,null]}"#);
//! ```
//!
//! Objects keep their keys in document order. Lookups on an object with a duplicated key return
//! the last occurrence, matching the usual "last one wins" reading of such documents.

use std::fmt;

/// Nesting deeper than this is rejected rather than risking a stack overflow on hostile input.
const MAX_DEPTH: usize = 128;

/// A parsed JSON value.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    /// Key/value pairs in document order.
    Object(Vec<(String, Value)>),
}

static NULL: Value = Value::Null;

/// Parse failure, with the byte offset at which it was detected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for Error {}

impl Value {
    /// Parse one JSON document. Leading and trailing whitespace is allowed; anything else after
    /// the value is an error.
    pub fn parse(s: &str) -> Result<Value, Error> {
        let mut p = Parser {
            b: s.as_bytes(),
            s,
            i: 0,
        };
        p.ws();
        let v = p.value(0)?;
        p.ws();
        if p.i != p.b.len() {
            return Err(p.err("trailing characters"));
        }
        Ok(v)
    }

    /// Member of an object, or `None` if this is not an object or has no such key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }
    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(m) => Some(m),
            _ => None,
        }
    }
}

/// `value["key"]` — the member, or `Null` if absent or if this is not an object.
impl std::ops::Index<&str> for Value {
    type Output = Value;
    fn index(&self, key: &str) -> &Value {
        self.get(key).unwrap_or(&NULL)
    }
}

/// `value[i]` — the element, or `Null` if out of range or if this is not an array.
impl std::ops::Index<usize> for Value {
    type Output = Value;
    fn index(&self, i: usize) -> &Value {
        self.as_array().and_then(|a| a.get(i)).unwrap_or(&NULL)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(s)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}
impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Value::Number(n as f64)
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(o: Option<T>) -> Self {
        o.map_or(Value::Null, Into::into)
    }
}
impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

/// Compact JSON, keys in insertion order. Non-finite numbers are written as `null`, since JSON
/// has no representation for them.
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        self.write_to(&mut out);
        f.write_str(&out)
    }
}

impl Value {
    /// Append compact JSON text to `out`.
    pub fn write_to(&self, out: &mut String) {
        match self {
            Value::Null => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Number(n) => write_number(out, *n),
            Value::String(s) => write_string(out, s),
            Value::Array(a) => {
                out.push('[');
                for (i, v) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    v.write_to(out);
                }
                out.push(']');
            }
            Value::Object(m) => {
                out.push('{');
                for (i, (k, v)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(out, k);
                    out.push(':');
                    v.write_to(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_number(out: &mut String, n: f64) {
    use fmt::Write;
    let milli = n * 1000.0;
    if !n.is_finite() {
        out.push_str("null");
    } else if n.fract() == 0.0 && n.abs() < 1e15 {
        // Exact for every integer below 2^53; `-0.0` prints as `0`, as JavaScript writes it.
        let _ = write!(out, "{}", n as i64);
    } else if milli.fract() == 0.0 && milli.abs() < 1e15 && milli / 1000.0 == n {
        // At most three decimals (confidences and weights are rounded to them). Below 1e12 a
        // double's spacing is far finer than 0.001, so these digits are also the shortest string
        // that reads back as `n`: exactly what `Display` would write, without its general search.
        let k = milli as i64;
        let (int, frac) = (k.unsigned_abs() / 1000, k.unsigned_abs() % 1000);
        let sign = if k < 0 { "-" } else { "" };
        let digits = [
            b'0' + (frac / 100) as u8,
            b'0' + (frac / 10 % 10) as u8,
            b'0' + (frac % 10) as u8,
        ];
        let len = 3 - digits.iter().rev().take_while(|&&d| d == b'0').count();
        let _ = write!(out, "{sign}{int}.");
        out.push_str(std::str::from_utf8(&digits[..len]).unwrap_or_default());
    } else {
        // Rust's `Display` for f64 is the shortest string that round-trips and never uses
        // exponent notation, so it is always valid JSON.
        let _ = write!(out, "{n}");
    }
}

/// Bytes that need escaping in a JSON string: controls, `"` and `\\`.
const ESCAPE: [bool; 256] = {
    let mut t = [false; 256];
    let mut i = 0;
    while i < 0x20 {
        t[i] = true;
        i += 1;
    }
    t[b'"' as usize] = true;
    t[b'\\' as usize] = true;
    t
};

/// Offset of the first byte at or after `from` that needs escaping. Tests eight bytes at a time:
/// the classic "has a zero byte" / "has a byte less than n" word tricks can only misfire on bytes
/// after a true hit, so the lowest flagged byte is always the right answer.
fn next_escape(b: &[u8], mut from: usize) -> Option<usize> {
    const ONES: u64 = u64::from_ne_bytes([0x01; 8]);
    const HIGH: u64 = u64::from_ne_bytes([0x80; 8]);
    let zero_byte = |x: u64| x.wrapping_sub(ONES) & !x & HIGH;
    while let Some(chunk) = b.get(from..from + 8) {
        let x = u64::from_le_bytes(chunk.try_into().unwrap_or_default());
        let hit = (x.wrapping_sub(ONES * 0x20) & !x & HIGH)
            | zero_byte(x ^ (ONES * u64::from(b'"')))
            | zero_byte(x ^ (ONES * u64::from(b'\\')));
        if hit != 0 {
            return Some(from + hit.trailing_zeros() as usize / 8);
        }
        from += 8;
    }
    b[from..]
        .iter()
        .position(|&c| ESCAPE[c as usize])
        .map(|k| from + k)
}

fn write_string(out: &mut String, s: &str) {
    use fmt::Write;
    out.reserve(s.len() + 2);
    out.push('"');
    let b = s.as_bytes();
    let mut start = 0;
    while let Some(i) = next_escape(b, start) {
        out.push_str(&s[start..i]);
        match b[i] {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x08 => out.push_str("\\b"),
            0x0c => out.push_str("\\f"),
            c => {
                let _ = write!(out, "\\u{c:04x}");
            }
        }
        start = i + 1;
    }
    out.push_str(&s[start..]);
    out.push('"');
}

/// Types that serialise to JSON.
///
/// Implementors write compact JSON text straight into a buffer; [`ToJson::to_json_value`] is
/// available for callers that want to inspect or recombine the result as a [`Value`].
pub trait ToJson {
    /// Append compact JSON text to `out`.
    fn write_json(&self, out: &mut String);
    /// Compact JSON text.
    fn to_json(&self) -> String {
        let mut out = String::with_capacity(1024);
        self.write_json(&mut out);
        out
    }
    /// The same document as a [`Value`] tree.
    fn to_json_value(&self) -> Value {
        Value::parse(&self.to_json()).expect("ToJson writes valid JSON")
    }
}

impl ToJson for Value {
    fn write_json(&self, out: &mut String) {
        self.write_to(out);
    }
    fn to_json_value(&self) -> Value {
        self.clone()
    }
}
impl ToJson for str {
    fn write_json(&self, out: &mut String) {
        write_string(out, self);
    }
}
impl ToJson for String {
    fn write_json(&self, out: &mut String) {
        write_string(out, self);
    }
}
impl ToJson for bool {
    fn write_json(&self, out: &mut String) {
        out.push_str(if *self { "true" } else { "false" });
    }
}
impl ToJson for f64 {
    fn write_json(&self, out: &mut String) {
        write_number(out, *self);
    }
}
impl ToJson for usize {
    fn write_json(&self, out: &mut String) {
        use fmt::Write;
        let _ = write!(out, "{self}");
    }
}
/// `None` is written as `null`; use [`ObjectWriter::opt`] to omit the member instead.
impl<T: ToJson> ToJson for Option<T> {
    fn write_json(&self, out: &mut String) {
        match self {
            Some(v) => v.write_json(out),
            None => out.push_str("null"),
        }
    }
}
impl<T: ToJson> ToJson for [T] {
    fn write_json(&self, out: &mut String) {
        out.push('[');
        for (i, v) in self.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            v.write_json(out);
        }
        out.push(']');
    }
}
impl<T: ToJson> ToJson for Vec<T> {
    fn write_json(&self, out: &mut String) {
        self.as_slice().write_json(out);
    }
}
impl<T: ToJson + ?Sized> ToJson for &T {
    fn write_json(&self, out: &mut String) {
        (**self).write_json(out);
    }
}

/// Writes an object's members straight into a buffer, in call order.
///
/// ```
/// use traffic_sources::json::ObjectWriter;
///
/// let mut out = String::new();
/// ObjectWriter::new(&mut out).field("a", &1usize).opt("b", None::<&str>).finish();
/// assert_eq!(out, r#"{"a":1}"#);
/// ```
pub struct ObjectWriter<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> ObjectWriter<'a> {
    /// Start an object at the end of `out`.
    pub fn new(out: &'a mut String) -> Self {
        out.push('{');
        Self { out, first: true }
    }
    /// Write one member.
    pub fn field(mut self, k: &str, v: &(impl ToJson + ?Sized)) -> Self {
        if !self.first {
            self.out.push(',');
        }
        self.first = false;
        write_string(self.out, k);
        self.out.push(':');
        v.write_json(self.out);
        self
    }
    /// Omit the member entirely when the value is `None`.
    pub fn opt(self, k: &str, v: Option<&(impl ToJson + ?Sized)>) -> Self {
        match v {
            Some(v) => self.field(k, v),
            None => self,
        }
    }
    /// Close the object.
    pub fn finish(self) {
        self.out.push('}');
    }
}

struct Parser<'a> {
    b: &'a [u8],
    s: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn err(&self, m: &str) -> Error {
        Error {
            message: m.to_string(),
            offset: self.i,
        }
    }

    fn ws(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.b.get(self.i) {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &str) -> Result<(), Error> {
        if self.b[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            Ok(())
        } else {
            Err(self.err(&format!("expected `{lit}`")))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Value, Error> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        match self.b.get(self.i) {
            None => Err(self.err("unexpected end of input")),
            Some(b'n') => self.eat("null").map(|_| Value::Null),
            Some(b't') => self.eat("true").map(|_| Value::Bool(true)),
            Some(b'f') => self.eat("false").map(|_| Value::Bool(false)),
            Some(b'"') => self.string().map(Value::String),
            Some(b'[') => {
                self.i += 1;
                let mut a = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b']') {
                    self.i += 1;
                    return Ok(Value::Array(a));
                }
                loop {
                    self.ws();
                    a.push(self.value(depth + 1)?);
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b']') => {
                            self.i += 1;
                            return Ok(Value::Array(a));
                        }
                        _ => return Err(self.err("expected `,` or `]`")),
                    }
                }
            }
            Some(b'{') => {
                self.i += 1;
                let mut m = Vec::new();
                self.ws();
                if self.b.get(self.i) == Some(&b'}') {
                    self.i += 1;
                    return Ok(Value::Object(m));
                }
                loop {
                    self.ws();
                    if self.b.get(self.i) != Some(&b'"') {
                        return Err(self.err("expected a string key"));
                    }
                    let k = self.string()?;
                    self.ws();
                    self.eat(":")?;
                    self.ws();
                    let v = self.value(depth + 1)?;
                    m.push((k, v));
                    self.ws();
                    match self.b.get(self.i) {
                        Some(b',') => self.i += 1,
                        Some(b'}') => {
                            self.i += 1;
                            return Ok(Value::Object(m));
                        }
                        _ => return Err(self.err("expected `,` or `}`")),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.err("unexpected character")),
        }
    }

    fn number(&mut self) -> Result<Value, Error> {
        let start = self.i;
        let digits = |p: &mut Self| {
            let s = p.i;
            while p.b.get(p.i).is_some_and(u8::is_ascii_digit) {
                p.i += 1;
            }
            p.i - s
        };
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.b.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                digits(self);
            }
            _ => return Err(self.err("invalid number")),
        }
        if self.b.get(self.i) == Some(&b'.') {
            self.i += 1;
            if digits(self) == 0 {
                return Err(self.err("invalid number"));
            }
        }
        if let Some(b'e' | b'E') = self.b.get(self.i) {
            self.i += 1;
            if let Some(b'+' | b'-') = self.b.get(self.i) {
                self.i += 1;
            }
            if digits(self) == 0 {
                return Err(self.err("invalid number"));
            }
        }
        self.s[start..self.i]
            .parse::<f64>()
            .map(Value::Number)
            .map_err(|_| self.err("invalid number"))
    }

    fn hex4(&mut self) -> Result<u32, Error> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| self.err("invalid \\u escape"))?;
        self.i += 4;
        Ok(u32::from_str_radix(h, 16).expect("validated hex"))
    }

    fn string(&mut self) -> Result<String, Error> {
        self.i += 1; // opening quote
        let mut out = String::new();
        loop {
            let run = self.i;
            self.i = next_escape(self.b, self.i).unwrap_or(self.b.len());
            // `i` only ever stops on an ASCII byte, so this slice is on char boundaries.
            out.push_str(&self.s[run..self.i]);
            match self.b.get(self.i) {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let c = match self.b.get(self.i) {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{08}',
                        Some(b'f') => '\u{0c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => {
                            self.i += 1;
                            let hi = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&hi) {
                                if !self.b[self.i..].starts_with(b"\\u") {
                                    return Err(self.err("unpaired surrogate"));
                                }
                                self.i += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err(self.err("unpaired surrogate"));
                                }
                                0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return Err(self.err("unpaired surrogate"));
                            } else {
                                hi
                            };
                            out.push(
                                char::from_u32(cp).ok_or_else(|| self.err("invalid code point"))?,
                            );
                            continue;
                        }
                        _ => return Err(self.err("invalid escape")),
                    };
                    self.i += 1;
                    out.push(c);
                }
                Some(_) => return Err(self.err("control character in string")),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_kind_of_value() {
        let v = Value::parse(
            r#" { "a": null, "b": true, "c": false, "d": -12.5e1, "e": "x\"\\\/\b\f\n\r\t\u00e9\ud83d\ude80", "f": [], "g": {} } "#,
        )
        .unwrap();
        assert!(v["a"].is_null());
        assert_eq!(v["b"].as_bool(), Some(true));
        assert_eq!(v["c"].as_bool(), Some(false));
        assert_eq!(v["d"].as_f64(), Some(-125.0));
        assert_eq!(v["e"].as_str(), Some("x\"\\/\u{8}\u{c}\n\r\té🚀"));
        assert_eq!(v["f"].as_array().map(<[_]>::len), Some(0));
        assert_eq!(v["g"].as_object().map(<[_]>::len), Some(0));
    }

    #[test]
    fn numbers_follow_the_json_grammar() {
        for ok in [
            "0",
            "-0",
            "1",
            "-1",
            "0.5",
            "1e3",
            "1E-3",
            "1.5e+2",
            "123456789012345678901234567890",
        ] {
            assert!(Value::parse(ok).is_ok(), "{ok}");
        }
        for bad in [
            "01", "+1", ".5", "1.", "1e", "1e+", "-", "--1", "0x10", "NaN", "Infinity", "1_000",
        ] {
            assert!(Value::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn rejects_malformed_documents() {
        for bad in [
            "",
            " ",
            "{",
            "}",
            "[",
            "[1,]",
            "[,1]",
            "{\"a\":1,}",
            "{a:1}",
            "{\"a\" 1}",
            "\"unterminated",
            "\"bad \\x escape\"",
            "\"\\u12\"",
            "\"\\ud800\"",
            "\"\\udc00\"",
            "\"\\ud800\\u0041\"",
            "\"tab\there\"",
            "nul",
            "truth",
            "[1] 2",
            "{\"a\":1}}",
        ] {
            assert!(Value::parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_stack_overflow() {
        let deep = "[".repeat(100_000);
        assert!(Value::parse(&deep).unwrap_err().message.contains("deep"));
        let ok = "[".repeat(MAX_DEPTH) + &"]".repeat(MAX_DEPTH);
        assert!(Value::parse(&ok).is_ok());
    }

    #[test]
    fn errors_report_the_offset() {
        let e = Value::parse("[1, 2, x]").unwrap_err();
        assert_eq!(e.offset, 7);
        assert!(e.to_string().contains("byte 7"));
    }

    #[test]
    fn last_duplicate_key_wins_and_order_is_kept() {
        let v = Value::parse(r#"{"k": 1, "z": 0, "k": 2}"#).unwrap();
        assert_eq!(v["k"].as_f64(), Some(2.0));
        assert_eq!(v.to_string(), r#"{"k":1,"z":0,"k":2}"#);
    }

    #[test]
    fn indexing_never_panics() {
        let v = Value::parse("[1]").unwrap();
        assert!(v[5].is_null());
        assert!(v["k"].is_null());
        assert!(v[0]["k"][3].is_null());
    }

    #[test]
    fn serialisation_round_trips() {
        for src in [
            r#"{"a":[1,2.5,-0.001,true,false,null],"b":{"c":"d"}}"#,
            r#""quote \" backslash \\ newline \n tab \t nul \u0000 bell \u0007 é 🚀""#,
            r#"[0.997,0.515,0.1,1e-7,12345678901234567890]"#,
        ] {
            let v = Value::parse(src).unwrap();
            let again = Value::parse(&v.to_string()).unwrap();
            assert_eq!(v, again, "{src}");
        }
    }

    #[test]
    fn escape_scan_finds_the_first_escapable_byte() {
        for len in 0..24 {
            for at in 0..=len {
                for &c in &[
                    0u8, 0x1f, b'"', b'\\', 0x20, 0x21, 0x5b, 0x7f, 0x80, 0xa2, 0xdc, 0xff,
                ] {
                    let mut b = vec![b'a'; len];
                    if at < len {
                        b[at] = c;
                        // Bytes after the first hit must not change the answer.
                        b[len - 1] = if at + 1 < len { b'"' } else { c };
                    }
                    let want = b.iter().position(|&c| ESCAPE[c as usize]);
                    assert_eq!(next_escape(&b, 0), want, "{b:?}");
                }
            }
        }
    }

    #[test]
    fn short_decimals_print_exactly_as_display_does() {
        let mut values: Vec<f64> = (-3000..=3000).map(|k| k as f64 / 1000.0).collect();
        values.extend((0..=1000).map(|k| (k as f64 * 0.001).clamp(0.0, 1.0)));
        values.extend([
            0.1 + 0.2,
            1.0 / 3.0,
            123456789.125,
            999999999999.999,
            1e12 + 0.5,
            2.5e-4,
        ]);
        values.extend((0..500).map(|i| ((i as f64 * 0.7919).sin() * 1000.0).round() / 1000.0));
        for n in values {
            let mut out = String::new();
            write_number(&mut out, n);
            let want = if n == 0.0 {
                "0".to_string()
            } else {
                format!("{n}")
            };
            assert_eq!(out, want, "{n:?}");
        }
    }

    #[test]
    fn numbers_serialise_compactly() {
        let s = |n: f64| Value::Number(n).to_string();
        assert_eq!(s(0.997), "0.997");
        assert_eq!(s(1.0), "1");
        assert_eq!(s(0.0), "0");
        assert_eq!(s(-3.0), "-3");
        assert_eq!(s(3.0e20), "300000000000000000000");
        assert_eq!(s(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(s(f64::NAN), "null");
        assert_eq!(s(f64::INFINITY), "null");
        assert_eq!(s(1e300).parse::<f64>().unwrap(), 1e300);
    }

    #[test]
    fn strings_escape_only_what_json_requires() {
        assert_eq!(Value::from("a/b é").to_string(), "\"a/b é\"");
        assert_eq!(Value::from("\u{1}\u{1f}").to_string(), "\"\\u0001\\u001f\"");
        assert_eq!(Value::from("\u{7f}").to_string(), "\"\u{7f}\"");
    }

    #[test]
    fn object_writer_skips_absent_optionals() {
        let mut out = String::new();
        ObjectWriter::new(&mut out)
            .field("a", &1usize)
            .opt("b", None::<&String>)
            .opt("c", Some("x"))
            .field("d", &None::<String>)
            .field("e", &vec![0.5, -3.0])
            .finish();
        assert_eq!(out, r#"{"a":1,"c":"x","d":null,"e":[0.5,-3]}"#);
        assert_eq!(Value::parse(&out).unwrap().to_json(), out);
    }
}
