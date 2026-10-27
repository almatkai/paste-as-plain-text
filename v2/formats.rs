//! Source-compatible text regexes and validator options; JSON5 uses a native parser.
use crate::Preferences;
use regex::Regex;
use serde_json::Value;
use std::sync::OnceLock;

pub fn js_whitespace(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
pub fn transform(input: &str, p: Preferences) -> String {
    let input = if p.trim_start {
        input.trim_start_matches(js_whitespace)
    } else {
        input
    };
    let input = if p.trim_end {
        input.trim_end_matches(js_whitespace)
    } else {
        input
    };
    if p.clean_line_breaks {
        static LINES: OnceLock<Regex> = OnceLock::new();
        LINES
            .get_or_init(|| Regex::new(r"[ \t]*[\r\n]+[ \t]*").unwrap())
            .replace_all(input, " ")
            .into_owned()
    } else {
        input.into()
    }
}
pub fn extract_url(input: &str) -> Option<&str> {
    let start = input.match_indices("http").find_map(|(i, _)| {
        (input[i..].starts_with("http://") || input[i..].starts_with("https://"))
            .then_some(i)
            .filter(|i| {
                input[*i..]
                    .split_once("://")
                    .is_some_and(|(_, tail)| tail.chars().next().is_some_and(|c| !js_whitespace(c)))
            })
    })?;
    let end = input[start..]
        .find(js_whitespace)
        .map_or(input.len(), |i| start + i);
    Some(&input[start..end])
}
pub fn extract_number(input: &str) -> Option<String> {
    static NUMBER: OnceLock<Regex> = OnceLock::new();
    NUMBER
        .get_or_init(|| Regex::new(r"-?[0-9,]+(?:\.[0-9]+)?(?-u:\b)").unwrap())
        .find(input)
        .map(|m| m.as_str().replace(',', ""))
}

/// Equivalent validator.js isURL options from the source, without URL normalization.
/// Validation deliberately permits original path punctuation and preserves the input.
pub fn valid_markdown_url(input: &str) -> bool {
    if input.is_empty()
        || input.encode_utf16().count() > 2084
        || input
            .chars()
            .any(|c| js_whitespace(c) || c == '<' || c == '>')
    {
        return false;
    }
    let authority = input.split(['#', '?']).next().unwrap_or("");
    let Some((protocol, rest)) = authority.split_once(':') else {
        return false;
    };
    if !["http", "https", "ftp"].contains(&protocol.to_ascii_lowercase().as_str()) {
        return false;
    }
    // With required protocol, validator rejects no-slash strings that resemble
    // auth (http:user@host) or a numeric port (http:127.0.0.1).
    if !rest.starts_with("//") {
        let before_slash = rest.split('/').next().unwrap_or("");
        if let Some((before_at, _)) = before_slash.split_once('@') {
            if before_at
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.%:".contains(&b))
            {
                return false;
            }
        } else if rest.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            return false;
        }
    }
    // validator 13.15.23 accepts http:example.com, as well as http://example.com.
    let authority = rest
        .strip_prefix("//")
        .unwrap_or(rest)
        .split('/')
        .next()
        .unwrap_or("");
    let hostname = if let Some((auth, host)) = authority.split_once('@') {
        if auth.is_empty() || auth.split(':').count() > 2 || auth == ":" {
            return false;
        }
        host
    } else {
        authority
    };
    let (host, port, ipv6) = if let Some(rest) = hostname.strip_prefix('[') {
        let Some((host, suffix)) = rest.split_once(']') else {
            return false;
        };
        let port = if suffix.is_empty() {
            None
        } else if let Some(p) = suffix
            .strip_prefix(':')
            .filter(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit()))
        {
            Some(p)
        } else {
            return false;
        };
        (host, port, true)
    } else {
        let (host, port) = hostname
            .split_once(':')
            .map_or((hostname, None), |(h, p)| (h, Some(p)));
        (host, port, false)
    };
    if let Some(port) = port.filter(|p| !p.is_empty()) {
        if !port.bytes().all(|b| b.is_ascii_digit())
            || port.parse::<u32>().map_or(true, |p| p == 0 || p > 65535)
        {
            return false;
        }
    }
    if ipv6 {
        let (ip, zone) = host
            .split_once('%')
            .map_or((host, None), |(ip, z)| (ip, Some(z)));
        return zone.is_none_or(|z| {
            !z.is_empty() && z.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.')
        }) && ip.parse::<std::net::Ipv6Addr>().is_ok();
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    static TLD: OnceLock<Regex> = OnceLock::new();
    let Some((_, tld)) = host.rsplit_once('.') else {
        return false;
    };
    if !TLD.get_or_init(|| Regex::new(r"(?i)^([a-z\x{00a1}-\x{00a8}\x{00aa}-\x{d7ff}\x{f900}-\x{fdcf}\x{fdf0}-\x{ffef}]{2,}|xn[a-z0-9-]{2,})$").unwrap()).is_match(tld) { return false; }
    host.split('.').all(|part| {
        !part.is_empty()
            && part.encode_utf16().count() <= 63
            && !part.starts_with('-')
            && !part.ends_with('-')
            && part.encode_utf16().all(|c| {
                ((c <= 127 && (c as u8).is_ascii_alphanumeric())
                    || c == 45
                    || (0x00a1..=0xffff).contains(&c))
                    && !(0xff01..=0xff5e).contains(&c)
            })
    })
}

/// fileURLToPath failures leave the original string untouched, as in the source.
pub fn file_url_path(input: &str) -> Option<String> {
    let url = url::Url::parse(input).ok()?;
    if url.scheme() != "file" {
        return None;
    }
    // Node rejects encoded path separators, whereas url::to_file_path decodes them.
    let path = url.path().to_ascii_lowercase();
    let mut bytes = path.bytes();
    while let Some(b) = bytes.next() {
        if b == b'%'
            && (!bytes.next().is_some_and(|b| b.is_ascii_hexdigit())
                || !bytes.next().is_some_and(|b| b.is_ascii_hexdigit()))
        {
            return None;
        }
    }
    if path.contains("%2f") || (cfg!(windows) && path.contains("%5c")) {
        return None;
    }
    url.to_file_path().ok()?.into_os_string().into_string().ok()
}

pub fn pretty_json5(input: &str) -> Option<String> {
    // Bound parser recursion/input; excessive inputs fall back to source text.
    if input.len() > 1024 * 1024 || !bounded_nesting(input) {
        return None;
    }
    let value: Value = json5::from_str(input).ok()?;
    let mut out = String::new();
    write_js_json(&value, 0, &mut out)?;
    Some(out)
}
fn array_index(s: &str) -> Option<u32> {
    s.parse::<u32>()
        .ok()
        .filter(|v| *v != u32::MAX && v.to_string() == s)
}
fn write_js_json(value: &Value, depth: usize, out: &mut String) -> Option<()> {
    if depth > 128 {
        return None;
    }
    match value {
        Value::Number(n) => {
            let n = n.as_f64()?;
            if n.is_finite() {
                out.push_str(ryu_js::Buffer::new().format(n));
            } else {
                out.push_str("null");
            }
        }
        Value::Array(v) if !v.is_empty() => {
            out.push_str("[\n");
            for (i, value) in v.iter().enumerate() {
                out.push_str(&"  ".repeat(depth + 1));
                write_js_json(value, depth + 1, out)?;
                if i + 1 < v.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&"  ".repeat(depth));
            out.push(']');
        }
        Value::Object(v) if !v.is_empty() => {
            let mut entries: Vec<_> = v.iter().collect();
            // Stable sort puts ECMAScript array index keys first, retaining other insertion order.
            entries.sort_by_key(|(k, _)| array_index(k).map_or((1, 0), |i| (0, i)));
            out.push_str("{\n");
            for (i, (key, value)) in entries.iter().enumerate() {
                out.push_str(&"  ".repeat(depth + 1));
                out.push_str(&serde_json::to_string(key).ok()?);
                out.push_str(": ");
                write_js_json(value, depth + 1, out)?;
                if i + 1 < entries.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&"  ".repeat(depth));
            out.push('}');
        }
        _ => out.push_str(&serde_json::to_string(value).ok()?),
    }
    Some(())
}

// Scan only for resource bounds; the JSON5 parser remains the syntax authority.
fn bounded_nesting(input: &str) -> bool {
    let mut chars = input.chars().peekable();
    let mut depth: usize = 0;
    while let Some(c) = chars.next() {
        match c {
            '\'' | '"' => {
                while let Some(q) = chars.next() {
                    if q == '\\' {
                        chars.next();
                    } else if q == c {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for q in chars.by_ref() {
                    if matches!(q, '\r' | '\n' | '\u{2028}' | '\u{2029}') {
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                while let Some(q) = chars.next() {
                    if q == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            }
            '[' | '{' => {
                depth += 1;
                if depth > 64 {
                    return false;
                }
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    true
}
