//! Absolute-URL parsing after the WHATWG URL Standard (url.spec.whatwg.org), reduced to what
//! attribution reads: scheme, host, path, query and `application/x-www-form-urlencoded` pairs.
//!
//! Inputs that a browser would reject are rejected here too (empty or forbidden host, bad port,
//! malformed IPv4/IPv6), because a referrer that does not parse is evidence of nothing. Hosts are
//! normalised the way browsers serialise them: lowercased, IPv4 in dotted decimal, IPv6
//! compressed, and non-ASCII labels in punycode. Fragments are dropped.
//!
//! Not implemented: relative resolution against a base, and full UTS #46 mapping for non-ASCII
//! hosts (labels are lowercased and punycode-encoded, which covers ordinary IDNs; compatibility
//! characters such as fullwidth letters are not mapped).

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Url {
    scheme: String,
    host: Option<String>,
    path: String,
    query: Option<String>,
}

const SPECIAL: [&str; 6] = ["http", "https", "ws", "wss", "ftp", "file"];

/// Bytes outside printable ASCII are always encoded.
fn c0(b: u8) -> bool {
    !(0x20..=0x7E).contains(&b)
}
fn query_set(b: u8, special: bool) -> bool {
    c0(b) || matches!(b, b' ' | b'"' | b'#' | b'<' | b'>') || (special && b == b'\'')
}
fn path_set(b: u8) -> bool {
    query_set(b, false) || matches!(b, b'?' | b'`' | b'{' | b'}')
}

fn ascii(r: &[u8]) -> &str {
    std::str::from_utf8(r).expect("verbatim runs are ASCII")
}

fn encode(s: &str, set: impl Fn(u8) -> bool, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    // Every set encodes all non-ASCII bytes, so the runs copied verbatim are ASCII.
    let bytes = s.as_bytes();
    let mut run = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if set(b) {
            out.push_str(ascii(&bytes[run..i]));
            out.push('%');
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 15) as usize] as char);
            run = i + 1;
        }
    }
    out.push_str(ascii(&bytes[run..]));
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

fn percent_decode(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' && i + 2 < s.len() {
            if let (Some(h), Some(l)) = (hex(s[i + 1]), hex(s[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

impl Url {
    pub(crate) fn parse(input: &str) -> Option<Url> {
        let trimmed = input.trim_matches(|c: char| c <= ' ');
        let cleaned: String;
        let s = if trimmed.contains(['\t', '\n', '\r']) {
            cleaned = trimmed.replace(['\t', '\n', '\r'], "");
            cleaned.as_str()
        } else {
            trimmed
        };
        let colon = s.find(':')?;
        let scheme = &s[..colon];
        let mut sc = scheme.bytes();
        if !sc.next()?.is_ascii_alphabetic()
            || !sc.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        {
            return None;
        }
        let scheme = scheme.to_ascii_lowercase();
        let rest = &s[colon + 1..];
        let rest = rest.split_once('#').map_or(rest, |(a, _)| a);
        let (rest, raw_query) = match rest.split_once('?') {
            Some((a, q)) => (a, Some(q)),
            None => (rest, None),
        };
        let special = SPECIAL.contains(&scheme.as_str());
        let query = raw_query.map(|q| {
            let mut out = String::new();
            encode(q, |b| query_set(b, special), &mut out);
            out
        });
        let (host, path) = if scheme == "file" {
            file(rest)?
        } else if special {
            let r = rest.trim_start_matches(['/', '\\']);
            let end = r.find(['/', '\\']).unwrap_or(r.len());
            let host = authority(&r[..end], true)?;
            if host.is_empty() {
                return None;
            }
            (Some(host), path(&r[end..], true))
        } else if let Some(r) = rest.strip_prefix("//") {
            let end = r.find('/').unwrap_or(r.len());
            let host = authority(&r[..end], false)?;
            let p = &r[end..];
            (
                Some(host),
                if p.is_empty() {
                    String::new()
                } else {
                    path(p, false)
                },
            )
        } else if rest.starts_with('/') {
            (None, path(rest, false))
        } else {
            let mut out = String::new();
            encode(rest, c0, &mut out);
            (None, out)
        };
        Some(Url {
            scheme,
            host,
            path,
            query,
        })
    }

    pub(crate) fn scheme(&self) -> &str {
        &self.scheme
    }
    /// The serialised host; `None` when the URL has none (an empty host is `None` as well).
    pub(crate) fn host_str(&self) -> Option<&str> {
        self.host.as_deref().filter(|h| !h.is_empty())
    }
    pub(crate) fn path(&self) -> &str {
        &self.path
    }
    pub(crate) fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    /// The query as `application/x-www-form-urlencoded` name/value pairs.
    pub(crate) fn query_pairs(&self) -> Vec<(String, String)> {
        self.query.as_deref().map(form_pairs).unwrap_or_default()
    }
}

/// `application/x-www-form-urlencoded` parsing (url.spec.whatwg.org/#urlencoded-parsing).
pub(crate) fn form_pairs(q: &str) -> Vec<(String, String)> {
    let dec = |s: &str| {
        if !s.contains(['+', '%']) {
            return s.to_string();
        }
        let plus: Vec<u8> = s
            .bytes()
            .map(|b| if b == b'+' { b' ' } else { b })
            .collect();
        String::from_utf8_lossy(&percent_decode(&plus)).into_owned()
    };
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (dec(k), dec(v)),
            None => (dec(p), String::new()),
        })
        .collect()
}

fn file(rest: &str) -> Option<(Option<String>, String)> {
    let b = rest.as_bytes();
    let slash = |i: usize| matches!(b.get(i), Some(b'/' | b'\\'));
    let (host, p) = if slash(0) && slash(1) {
        let r = &rest[2..];
        let end = r.find(['/', '\\']).unwrap_or(r.len());
        let h = &r[..end];
        if is_drive(h) {
            (String::new(), format!("/{r}"))
        } else if h.is_empty() {
            (String::new(), r.to_string())
        } else {
            let h = host(h, true)?;
            let h = if h == "localhost" { String::new() } else { h };
            (h, r[end..].to_string())
        }
    } else if slash(0) {
        (String::new(), rest.to_string())
    } else {
        (String::new(), format!("/{rest}"))
    };
    let (segs, drive) = segments(&p, true, true);
    let host = if drive { String::new() } else { host };
    Some((Some(host), format!("/{}", segs.join("/"))))
}

fn is_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && matches!(b[1], b':' | b'|')
}

/// Host of an authority (`userinfo@host:port`), validating the port.
fn authority(a: &str, special: bool) -> Option<String> {
    let hp = a.rsplit_once('@').map_or(a, |(_, h)| h);
    if a.contains('@') && hp.is_empty() {
        return None;
    }
    // A ':' inside an IPv6 literal is not a port separator.
    let port_at = match hp.rfind(']') {
        Some(close) => hp[close..].find(':').map(|i| close + i),
        None => hp.find(':'),
    };
    let (h, port) = match port_at {
        Some(i) => (&hp[..i], Some(&hp[i + 1..])),
        None => (hp, None),
    };
    if let Some(p) = port {
        if !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if !p.is_empty() && p.trim_start_matches('0').len() > 5
            || p.parse::<u32>().is_ok_and(|n| n > 65535)
        {
            return None;
        }
        if h.is_empty() {
            return None;
        }
    }
    host(h, special)
}

const FORBIDDEN_HOST: &[u8] = b"\0\t\n\r #/:<>?@[\\]^|";

fn host(h: &str, special: bool) -> Option<String> {
    if let Some(v6) = h.strip_prefix('[') {
        return ipv6(v6.strip_suffix(']')?).map(|a| format!("[{}]", ipv6_string(a)));
    }
    if !special {
        if h.bytes().any(|b| FORBIDDEN_HOST.contains(&b)) {
            return None;
        }
        let mut out = String::new();
        encode(h, c0, &mut out);
        return Some(out);
    }
    if h.is_empty() {
        return Some(String::new());
    }
    // The common case: a plain ASCII name that needs no decoding, mapping or IPv4 parsing.
    let last = h.rsplit('.').find(|l| !l.is_empty()).unwrap_or("");
    if h.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_')
        && !last.starts_with(|c: char| c.is_ascii_digit())
        && !h
            .as_bytes()
            .windows(4)
            .any(|w| w.eq_ignore_ascii_case(b"xn--"))
    {
        return Some(h.to_ascii_lowercase());
    }
    // Invalid UTF-8 would decode to U+FFFD, which IDNA disallows, so the host fails either way.
    let decoded = String::from_utf8(percent_decode(h.as_bytes())).ok()?;
    let ascii = domain_to_ascii(&decoded)?;
    if ascii
        .bytes()
        .any(|b| FORBIDDEN_HOST.contains(&b) || b < 0x20 || b == b'%' || b == 0x7F)
    {
        return None;
    }
    if ends_in_number(&ascii) {
        return ipv4(&ascii).map(|a| {
            let o = a.to_be_bytes();
            format!("{}.{}.{}.{}", o[0], o[1], o[2], o[3])
        });
    }
    Some(ascii)
}

fn domain_to_ascii(d: &str) -> Option<String> {
    let mapped: String = if d.is_ascii() {
        d.to_ascii_lowercase()
    } else {
        d.chars()
            .flat_map(|c| match c as u32 {
                0x3002 | 0xFF0E | 0xFF61 => '.'.to_lowercase(),
                // Fullwidth ASCII, the compatibility mapping met most often in hosts.
                n @ 0xFF01..=0xFF5E => char::from_u32(n - 0xFEE0)
                    .expect("maps onto printable ASCII")
                    .to_lowercase(),
                _ => c.to_lowercase(),
            })
            .collect()
    };
    let labels: Option<Vec<String>> = mapped
        .split('.')
        .map(|l| {
            if l.starts_with("xn--") {
                // An A-label must decode, to plausible label characters, and to something that
                // needed encoding.
                let u = punycode_decode(l.strip_prefix("xn--")?)?;
                let ok = !u.is_empty()
                    && !u.iter().all(char::is_ascii)
                    && u.iter().all(|&c| label_char(c) && c.to_lowercase().eq([c]));
                ok.then(|| l.to_string())
            } else if !l.is_ascii() {
                if !l.chars().all(label_char) {
                    return None;
                }
                punycode(l).map(|p| format!("xn--{p}"))
            } else {
                Some(l.to_string())
            }
        })
        .collect();
    Some(labels?.join("."))
}

/// Approximates IDNA's "valid" status without its tables: ASCII is checked separately against
/// the forbidden host code points; beyond ASCII, letters, digits and the emoji blocks pass, and
/// controls, private use, U+FFFD and unassigned code points (which are not alphanumeric) fail.
fn label_char(c: char) -> bool {
    c.is_ascii() || c.is_alphanumeric() || matches!(c as u32, 0x2600..=0x27BF | 0x1F300..=0x1FAFF)
}

const BASE: u32 = 36;
const TMIN: u32 = 1;
const TMAX: u32 = 26;

fn adapt(mut delta: u32, num: u32, first: bool) -> u32 {
    delta /= if first { 700 } else { 2 };
    delta += delta / num;
    let mut k = 0;
    while delta > ((BASE - TMIN) * TMAX) / 2 {
        delta /= BASE - TMIN;
        k += BASE;
    }
    k + (BASE - TMIN + 1) * delta / (delta + 38)
}

fn threshold(k: u32, bias: u32) -> u32 {
    if k <= bias {
        TMIN
    } else if k >= bias + TMAX {
        TMAX
    } else {
        k - bias
    }
}

/// RFC 3492 decoding of one label (without the `xn--` prefix).
fn punycode_decode(s: &str) -> Option<Vec<char>> {
    let (basic, ext) = match s.rfind('-') {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => ("", s),
    };
    if !basic.is_ascii() {
        return None;
    }
    let mut out: Vec<u32> = basic.chars().map(|c| c as u32).collect();
    let (mut n, mut i, mut bias) = (0x80u32, 0u32, 72u32);
    let mut it = ext.bytes().peekable();
    while it.peek().is_some() {
        let old_i = i;
        let (mut w, mut k) = (1u32, BASE);
        loop {
            let d = match it.next()? {
                b @ b'a'..=b'z' => b - b'a',
                b @ b'A'..=b'Z' => b - b'A',
                b @ b'0'..=b'9' => b - b'0' + 26,
                _ => return None,
            } as u32;
            i = i.checked_add(d.checked_mul(w)?)?;
            let t = threshold(k, bias);
            if d < t {
                break;
            }
            w = w.checked_mul(BASE - t)?;
            k += BASE;
        }
        let len = out.len() as u32 + 1;
        bias = adapt(i - old_i, len, old_i == 0);
        n = n.checked_add(i / len)?;
        i %= len;
        out.insert(i as usize, n);
        i += 1;
    }
    out.into_iter().map(char::from_u32).collect()
}

/// RFC 3492 encoding of one label.
fn punycode(label: &str) -> Option<String> {
    let digit = |d: u32| {
        (if d < 26 {
            b'a' + d as u8
        } else {
            b'0' + (d - 26) as u8
        }) as char
    };
    let cps: Vec<u32> = label.chars().map(|c| c as u32).collect();
    let mut out: String = label.chars().filter(char::is_ascii).collect();
    let basic = out.len() as u32;
    let mut handled = basic;
    if basic > 0 {
        out.push('-');
    }
    let (mut n, mut delta, mut bias) = (0x80u32, 0u32, 72u32);
    while (handled as usize) < cps.len() {
        let m = *cps.iter().filter(|&&c| c >= n).min()?;
        delta = delta.checked_add((m - n).checked_mul(handled + 1)?)?;
        n = m;
        for &c in &cps {
            if c < n {
                delta = delta.checked_add(1)?;
            }
            if c == n {
                let mut q = delta;
                let mut k = BASE;
                loop {
                    let t = threshold(k, bias);
                    if q < t {
                        break;
                    }
                    out.push(digit(t + (q - t) % (BASE - t)));
                    q = (q - t) / (BASE - t);
                    k += BASE;
                }
                out.push(digit(q));
                bias = adapt(delta, handled + 1, handled == basic);
                delta = 0;
                handled += 1;
            }
        }
        delta += 1;
        n += 1;
    }
    Some(out)
}

fn ends_in_number(h: &str) -> bool {
    let mut parts: Vec<&str> = h.split('.').collect();
    if parts.last() == Some(&"") {
        if parts.len() == 1 {
            return false;
        }
        parts.pop();
    }
    let last = parts.last().copied().unwrap_or("");
    if !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    let l = last.as_bytes();
    l.len() >= 2
        && l[0] == b'0'
        && (l[1] | 0x20) == b'x'
        && l[2..].iter().all(u8::is_ascii_hexdigit)
}

fn ipv4_number(p: &str) -> Option<u64> {
    let (digits, radix) = if p.len() >= 2 && (p.starts_with("0x") || p.starts_with("0X")) {
        (&p[2..], 16)
    } else if p.len() >= 2 && p.starts_with('0') {
        (&p[1..], 8)
    } else {
        (p, 10)
    };
    if digits.is_empty() {
        return Some(0);
    }
    if digits.len() > 20 {
        // Too large for any IPv4 part; still must be all valid digits to count as a number.
        return digits
            .chars()
            .all(|c| c.is_digit(radix))
            .then_some(u64::MAX);
    }
    u64::from_str_radix(digits, radix).ok()
}

fn ipv4(h: &str) -> Option<u32> {
    let mut parts: Vec<&str> = h.split('.').collect();
    if parts.last() == Some(&"") && parts.len() > 1 {
        parts.pop();
    }
    if parts.len() > 4 {
        return None;
    }
    let nums: Vec<u64> = parts
        .iter()
        .map(|p| if p.is_empty() { None } else { ipv4_number(p) })
        .collect::<Option<_>>()?;
    let (last, init) = nums.split_last()?;
    if init.iter().any(|&n| n > 255) {
        return None;
    }
    if *last >= 256u64.pow(5 - nums.len() as u32) {
        return None;
    }
    let mut v = *last;
    for (i, &n) in init.iter().enumerate() {
        v += n << (8 * (3 - i));
    }
    u32::try_from(v).ok()
}

fn ipv6(s: &str) -> Option<[u16; 8]> {
    let b = s.as_bytes();
    let mut a = [0u16; 8];
    let (mut piece, mut compress, mut i) = (0usize, None, 0usize);
    if b.first() == Some(&b':') {
        if b.get(1) != Some(&b':') {
            return None;
        }
        i = 2;
        piece = 1;
        compress = Some(1);
    }
    while i < b.len() {
        if piece == 8 {
            return None;
        }
        if b[i] == b':' {
            if compress.is_some() {
                return None;
            }
            i += 1;
            piece += 1;
            compress = Some(piece);
            continue;
        }
        let (mut value, mut len) = (0u16, 0);
        while len < 4 && i < b.len() && b[i].is_ascii_hexdigit() {
            value = value * 16 + hex(b[i])? as u16;
            i += 1;
            len += 1;
        }
        if i < b.len() && b[i] == b'.' {
            if len == 0 || piece > 6 {
                return None;
            }
            i -= len;
            let mut seen = 0;
            while i < b.len() {
                if seen > 0 {
                    if b[i] == b'.' && seen < 4 {
                        i += 1;
                    } else {
                        return None;
                    }
                }
                if i >= b.len() || !b[i].is_ascii_digit() {
                    return None;
                }
                let mut v: Option<u16> = None;
                while i < b.len() && b[i].is_ascii_digit() {
                    let d = (b[i] - b'0') as u16;
                    v = match v {
                        None => Some(d),
                        Some(0) => return None,
                        Some(x) => Some(x * 10 + d),
                    };
                    if v? > 255 {
                        return None;
                    }
                    i += 1;
                }
                a[piece] = a[piece] * 0x100 + v?;
                seen += 1;
                if seen == 2 || seen == 4 {
                    piece += 1;
                }
            }
            if seen != 4 {
                return None;
            }
            break;
        }
        if i < b.len() && b[i] == b':' {
            i += 1;
            if i >= b.len() {
                return None;
            }
        } else if i < b.len() {
            return None;
        }
        a[piece] = value;
        piece += 1;
    }
    match compress {
        Some(c) => {
            let mut swaps = piece - c;
            piece = 7;
            while piece != 0 && swaps > 0 {
                a.swap(piece, c + swaps - 1);
                piece -= 1;
                swaps -= 1;
            }
        }
        None if piece != 8 => return None,
        None => {}
    }
    Some(a)
}

fn ipv6_string(a: [u16; 8]) -> String {
    // Compress the first longest run of two or more zero pieces.
    let (mut best, mut best_len, mut i) = (None, 1, 0);
    while i < 8 {
        if a[i] == 0 {
            let s = i;
            while i < 8 && a[i] == 0 {
                i += 1;
            }
            if i - s > best_len {
                best = Some(s);
                best_len = i - s;
            }
        } else {
            i += 1;
        }
    }
    let mut out = String::new();
    let mut i = 0;
    while i < 8 {
        if Some(i) == best {
            out.push_str(if i == 0 { "::" } else { ":" });
            i += best_len;
            continue;
        }
        out.push_str(&format!("{:x}", a[i]));
        if i < 7 {
            out.push(':');
        }
        i += 1;
    }
    out
}

/// Path with dot segments resolved and the path percent-encode set applied. `s` is empty or
/// starts with a separator.
fn path(s: &str, special: bool) -> String {
    let is_sep = |c: char| c == '/' || (special && c == '\\');
    let body = s.strip_prefix(is_sep).unwrap_or(s);
    let mut out = String::with_capacity(s.len() + 1);
    // Offsets in `out` of each segment's leading '/', so `..` can truncate.
    let mut starts: Vec<usize> = Vec::new();
    let mut parts = body.split(is_sep).peekable();
    while let Some(seg) = parts.next() {
        let last = parts.peek().is_none();
        match dot_segment(seg) {
            Some(2) => {
                if let Some(st) = starts.pop() {
                    out.truncate(st);
                }
                if last {
                    starts.push(out.len());
                    out.push('/');
                }
            }
            Some(_) => {
                if last {
                    starts.push(out.len());
                    out.push('/');
                }
            }
            None => {
                starts.push(out.len());
                out.push('/');
                encode(seg, path_set, &mut out);
            }
        }
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

/// `Some(1)` for `.`, `Some(2)` for `..`, in any of their percent-encoded spellings.
fn dot_segment(seg: &str) -> Option<u8> {
    if seg.len() > 6 || !seg.starts_with(['.', '%']) {
        return None;
    }
    match seg.to_ascii_lowercase().replace("%2e", ".").as_str() {
        "." => Some(1),
        ".." => Some(2),
        _ => None,
    }
}

/// Path segments, and whether the path starts with a Windows drive letter (`file:` only, where
/// that letter also displaces any host and survives `..`).
fn segments(s: &str, special: bool, file: bool) -> (Vec<String>, bool) {
    let is_sep = |c: char| c == '/' || (special && c == '\\');
    let mut segs: Vec<String> = Vec::new();
    let mut drive = false;
    let body = s.strip_prefix(is_sep).unwrap_or(s);
    let parts: Vec<&str> = body.split(is_sep).collect();
    let last = parts.len() - 1;
    for (i, seg) in parts.iter().enumerate() {
        let dots = dot_segment(seg);
        if dots == Some(2) {
            let keep_drive =
                file && segs.len() == 1 && is_drive(&segs[0]) && !segs[0].contains('|');
            if !keep_drive {
                segs.pop();
            }
            if i == last {
                segs.push(String::new());
            }
        } else if dots == Some(1) {
            if i == last {
                segs.push(String::new());
            }
        } else if file && segs.is_empty() && is_drive(seg) {
            drive = true;
            segs.push(format!("{}:", &seg[..1]));
        } else {
            let mut out = String::new();
            encode(seg, path_set, &mut out);
            segs.push(out);
        }
    }
    if file {
        while segs.len() > 1 && segs[0].is_empty() {
            segs.remove(0);
        }
    }
    (segs, drive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(u: &str) -> Option<String> {
        Url::parse(u).and_then(|u| u.host_str().map(String::from))
    }

    #[test]
    fn splits_scheme_host_path_and_query() {
        let u = Url::parse("HTTPS://user:pw@WWW.Example.com:8443/a/./b/../c?q=1#frag").unwrap();
        assert_eq!(u.scheme(), "https");
        assert_eq!(u.host_str(), Some("www.example.com"));
        assert_eq!(u.path(), "/a/c");
        assert_eq!(u.query(), Some("q=1"));
    }

    #[test]
    fn special_schemes_default_the_path_and_tolerate_slashes() {
        assert_eq!(Url::parse("https://a.example").unwrap().path(), "/");
        assert_eq!(host("https:a.example"), Some("a.example".into()));
        assert_eq!(host("https:\\\\a.example\\x"), Some("a.example".into()));
        assert_eq!(
            Url::parse("https://a.example\\x\\y").unwrap().path(),
            "/x/y"
        );
    }

    #[test]
    fn rejects_what_browsers_reject() {
        for bad in [
            "",
            "not a url",
            "/relative",
            "://x",
            "http://",
            "https://user@/",
            "https://a b/",
            "https://a.example:port/",
            "https://a.example:65536/",
            "https://[::1/",
            "1http://a/",
            "https://a<b/",
            "https://%zz/",
            "https://1.2.3.4.5/",
            "https://256.0.0.1/",
            "http://:80/",
        ] {
            assert_eq!(Url::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn hosts_are_normalised() {
        assert_eq!(host("http://0x7f.1/"), Some("127.0.0.1".into()));
        assert_eq!(host("http://2130706433/"), Some("127.0.0.1".into()));
        assert_eq!(host("http://[0:0:0:0:0:0:0:1]/"), Some("[::1]".into()));
        assert_eq!(
            host("http://[2001:DB8::0:1]/"),
            Some("[2001:db8::1]".into())
        );
        assert_eq!(
            host("http://[::ffff:1.2.3.4]/"),
            Some("[::ffff:102:304]".into())
        );
        assert_eq!(
            host("http://bücher.example/"),
            Some("xn--bcher-kva.example".into())
        );
        assert_eq!(host("http://%41.example/"), Some("a.example".into()));
        assert_eq!(host("http://a.example./"), Some("a.example.".into()));
    }

    #[test]
    fn non_special_schemes() {
        let u = Url::parse("android-app://com.Reddit.frontpage").unwrap();
        assert_eq!(
            u.host_str(),
            Some("com.Reddit.frontpage"),
            "opaque hosts keep case"
        );
        assert_eq!(u.path(), "");
        let u = Url::parse("mailto:a@b.example").unwrap();
        assert_eq!((u.host_str(), u.path()), (None, "a@b.example"));
        assert_eq!(Url::parse("foo:/x/../y").unwrap().path(), "/y");
    }

    #[test]
    fn percent_encoding() {
        let u = Url::parse("https://a.example/a b/é?x=a b&y='\"é").unwrap();
        assert_eq!(u.path(), "/a%20b/%C3%A9");
        assert_eq!(u.query(), Some("x=a%20b&y=%27%22%C3%A9"));
    }

    #[test]
    fn form_urlencoded_pairs() {
        let u = Url::parse("https://a.example/?a=1&&b=x+y%20z&c&=v&d=%zz&e=%C3%A9&f=1=2").unwrap();
        let p: Vec<(String, String)> = u.query_pairs();
        let s = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(
            p,
            vec![
                s("a", "1"),
                s("b", "x y z"),
                s("c", ""),
                s("", "v"),
                s("d", "%zz"),
                s("e", "é"),
                s("f", "1=2")
            ]
        );
        assert_eq!(form_pairs("x=%FF")[0].1, "\u{FFFD}");
    }

    #[test]
    fn punycode_vectors() {
        assert_eq!(punycode("bücher").as_deref(), Some("bcher-kva"));
        assert_eq!(punycode("münchen").as_deref(), Some("mnchen-3ya"));
        assert_eq!(punycode("例え").as_deref(), Some("r8jz45g"));
        for l in ["bücher", "münchen", "例え", "ñ", "aé-ü"] {
            let enc = punycode(l).unwrap();
            assert_eq!(punycode_decode(&enc).unwrap().iter().collect::<String>(), l);
        }
        assert_eq!(punycode_decode("zz!"), None);
        assert_eq!(host("http://xn--zz.de/"), None, "invalid A-label");
        assert_eq!(
            host("http://xn--abc-.de/"),
            None,
            "A-label that encodes only ASCII"
        );
        assert_eq!(
            host("http://ａｂ.de/"),
            Some("ab.de".into()),
            "fullwidth ASCII"
        );
    }
}
