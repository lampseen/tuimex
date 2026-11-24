//! What is known about a link before it opens (ADR-018).

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assessment {
    /// The target with known click-tracking parameters removed.
    pub url: String,
    pub host: String,
    /// Each one means the link is confirmed before it opens.
    pub warnings: Vec<Warning>,
    /// The scheme is not `http`, `https` or `mailto`: shown, never opened.
    pub blocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Warning {
    TextNamesAnotherHost,
    CarriesCredentials,
    BareIpAddress,
    MixedScripts,
}

/// A URL as far as its scheme and host, read the way a browser reads it
/// rather than the way it is spelled: the question is always where a browser
/// would go, and a stricter reading answers a different one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    /// With tabs, line breaks and surrounding spaces removed.
    pub text: String,
    /// Lower case; empty when the URL is relative.
    pub scheme: String,
    /// Lower case; empty when the URL names none.
    pub host: String,
    /// Something precedes an `@` in the authority.
    pub credentials: bool,
}

impl Url {
    pub fn parse(raw: &str) -> Url {
        // A browser trims a URL and drops tabs and line breaks anywhere in
        // it, so ` https://…` and `ht\ntps://…` are both https.
        let text: String = raw.trim_matches(|c: char| c <= ' ').chars().filter(|c| !c.is_control()).collect();
        let (scheme, rest) = match text.find(':') {
            Some(i) if is_scheme(&text[..i]) => (text[..i].to_ascii_lowercase(), &text[i + 1..]),
            _ => (String::new(), text.as_str()),
        };
        let authority = match scheme.as_str() {
            // For these a browser accepts any run of either slash, or none.
            "http" | "https" | "ftp" | "ws" | "wss" => Some(rest.trim_start_matches(['/', '\\'])),
            "mailto" => rest.split(['?', '#']).next(),
            _ => rest.strip_prefix("//").or_else(|| rest.strip_prefix("\\\\")),
        };
        let authority = authority.and_then(|a| a.split(['/', '\\', '?', '#']).next()).unwrap_or("");
        let (credentials, host_port) = match authority.rfind('@') {
            Some(i) => (scheme != "mailto", &authority[i + 1..]),
            // An address is the only authority a mailto has.
            None if scheme == "mailto" => (false, ""),
            None => (false, authority),
        };
        let host = match host_port.find(']') {
            Some(i) if host_port.starts_with('[') => &host_port[..=i],
            _ => host_port.split(':').next().unwrap_or(""),
        };
        Url { scheme, host: host.to_lowercase(), credentials, text: text.clone() }
    }

    /// Whether loading this would contact `host`: true of the web schemes
    /// and of a scheme-relative `//host/…`, which takes the page's.
    pub fn is_remote(&self) -> bool {
        !self.host.is_empty() && matches!(self.scheme.as_str(), "" | "http" | "https" | "ftp" | "ws" | "wss")
    }
}

fn is_scheme(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic())
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

pub fn assess(url: &str, shown: &str) -> Assessment {
    let target = Url::parse(url);
    let web = matches!(target.scheme.as_str(), "http" | "https");
    let mut warnings = Vec::new();
    if named_hosts(shown).any(|named| !same_site(&named, &target.host)) {
        warnings.push(Warning::TextNamesAnotherHost);
    }
    if web && target.credentials {
        warnings.push(Warning::CarriesCredentials);
    }
    if web && is_ip(&target.host) {
        warnings.push(Warning::BareIpAddress);
    }
    if web && target.host.split('.').any(mixes_scripts) {
        warnings.push(Warning::MixedScripts);
    }
    Assessment {
        url: if web { without_tracking(&target.text) } else { target.text },
        blocked: target.host.is_empty() || !(web || target.scheme == "mailto"),
        host: target.host,
        warnings,
    }
}

/// The hosts a link's text claims, whether as a URL, an address or a bare
/// domain. A file name such as `report.pdf` reads as one too, and the cost
/// of that is a confirmation.
fn named_hosts(shown: &str) -> impl Iterator<Item = String> + '_ {
    shown.split_whitespace().filter_map(|word| {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());
        let host = if word.contains("://") {
            Url::parse(word).host
        } else {
            let after_at = word.rsplit('@').next().unwrap_or(word);
            after_at.split(['/', '?', '#', ':']).next().unwrap_or("").to_lowercase()
        };
        let mut labels = host.split('.');
        let tld = labels.next_back()?;
        let label = |l: &str| !l.is_empty() && l.chars().all(|c| c.is_alphanumeric() || c == '-');
        let named = tld.chars().count() >= 2 && tld.chars().all(char::is_alphabetic) && labels.clone().count() >= 1;
        (named && labels.all(label)).then_some(host)
    })
}

/// One host is the other, or a subdomain of it: `example.com` on a link to
/// `www.example.com` deceives nobody.
fn same_site(a: &str, b: &str) -> bool {
    a == b || a.strip_suffix(b).is_some_and(|p| p.ends_with('.')) || b.strip_suffix(a).is_some_and(|p| p.ends_with('.'))
}

/// IPv6 in brackets, or IPv4 in any spelling a browser takes: `192.0.2.7`,
/// `0xc0.0.2.7`, `3221225991`.
fn is_ip(host: &str) -> bool {
    let number = |l: &str| {
        let hex = l.strip_prefix("0x");
        !l.is_empty() && hex.map_or(l.chars().all(|c| c.is_ascii_digit()), |h| h.chars().all(|c| c.is_ascii_hexdigit()))
    };
    host.starts_with('[') || (!host.is_empty() && host.split('.').all(number))
}

/// Letters of more than one script in a single label, which is how
/// `pаypal` with a Cyrillic `а` is spelled. A label that claims to be
/// punycode and is not counts, because nothing honest produces one.
fn mixes_scripts(label: &str) -> bool {
    let decoded = match label.strip_prefix("xn--") {
        Some(encoded) => match punycode(encoded) {
            Some(d) => d,
            None => return true,
        },
        None => label.to_string(),
    };
    let mut scripts = decoded.chars().filter(|c| c.is_alphabetic()).map(script);
    scripts.next().is_some_and(|first| scripts.any(|s| s != first))
}

/// A coarse script number: enough to tell Latin from what imitates it, and
/// to leave the scripts Japanese and Korean mix by nature in one class.
fn script(c: char) -> u32 {
    match c as u32 {
        0..=0x024F => 0,
        0x0370..=0x03FF => 1,
        0x0400..=0x052F => 2,
        0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF => 3,
        other => 4 + (other >> 8),
    }
}

/// RFC 3492, decoding only. `None` on anything malformed or overflowing.
fn punycode(input: &str) -> Option<String> {
    let (mut out, tail): (Vec<char>, &str) = match input.rfind('-') {
        Some(i) => (input[..i].chars().collect(), &input[i + 1..]),
        None => (Vec::new(), input),
    };
    let mut digits = tail.bytes().map(|b| match b {
        b'a'..=b'z' => Some(u32::from(b - b'a')),
        b'0'..=b'9' => Some(u32::from(b - b'0') + 26),
        _ => None,
    });
    let (mut n, mut i, mut bias, mut first) = (128u32, 0u32, 72u32, true);
    while let Some(digit) = digits.next() {
        let (start, mut weight, mut k, mut digit) = (i, 1u32, 36u32, digit?);
        loop {
            i = i.checked_add(digit.checked_mul(weight)?)?;
            let threshold = k.saturating_sub(bias).clamp(1, 26);
            if digit < threshold {
                break;
            }
            weight = weight.checked_mul(36 - threshold)?;
            k += 36;
            digit = digits.next()??;
        }
        // A label too long to count is malformed like any other.
        let len = u32::try_from(out.len()).ok()? + 1;
        let mut delta = (i - start) / if first { 700 } else { 2 };
        first = false;
        delta += delta / len;
        bias = 0;
        while delta > 455 {
            delta /= 35;
            bias += 36;
        }
        bias += 36 * delta / (delta + 38);
        n = n.checked_add(i / len)?;
        i %= len;
        out.insert(i as usize, char::from_u32(n)?);
        i += 1;
    }
    Some(out.into_iter().collect())
}

/// Query parameters that exist to report the click, not to find the page.
const TRACKING: [&str; 16] = [
    "fbclid",
    "gclid",
    "dclid",
    "gbraid",
    "wbraid",
    "msclkid",
    "yclid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "oly_anon_id",
    "oly_enc_id",
    "vero_id",
];

fn without_tracking(url: &str) -> String {
    let (before, fragment) = url.split_once('#').map_or((url, None), |(b, f)| (b, Some(f)));
    let Some((path, query)) = before.split_once('?') else { return url.to_string() };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            let name = pair.split('=').next().unwrap_or(pair).to_ascii_lowercase();
            !(name.starts_with("utm_") || TRACKING.contains(&name.as_str()))
        })
        .collect();
    let mut out = path.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = fragment {
        out.push('#');
        out.push_str(f);
    }
    out
}
