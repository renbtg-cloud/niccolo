//! Bounded, byte-exact extraction of attributed fragments from original sources.
//! Parsers don't infer speaker identity, truth, intent, or message authenticity.
//! The original source artifact must be retained in a separately authorized store.
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_FRAGMENTS: usize = 10_000;
pub const MAX_FRAGMENT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFormat {
    Eml,
    WhatsappExport,
}
impl SourceFormat {
    pub fn db_value(self) -> &'static str {
        match self {
            Self::Eml => "EML",
            Self::WhatsappExport => "WHATSAPP_EXPORT",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFragment {
    pub ordinal: i32,
    /// Half-open [start, end) range into the exact uploaded byte stream.
    pub byte_start: i32,
    pub byte_end: i32,
    /// Unverified string from a header or export file; never an authenticated identity.
    pub speaker_label: String,
    pub content: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSource {
    pub sha256: String,
    pub byte_length: i64,
    pub fragments: Vec<SourceFragment>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("source is empty or exceeds 4 MiB")]
    InvalidSize,
    #[error("source is not UTF-8")]
    NotUtf8,
    #[error("format or header is invalid/unsupported: {0}")]
    Unsupported(&'static str),
    #[error("source contains no attributable fragments")]
    Empty,
    #[error("source exceeds fragment count or size limits")]
    Limit,
}

fn iter_lines(src: &str) -> impl Iterator<Item = (usize, usize, &str)> {
    let mut offset = 0;
    std::iter::from_fn(move || {
        if offset >= src.len() {
            return None;
        }
        let start = offset;
        let stop = src[start..]
            .find('\n')
            .map(|v| start + v + 1)
            .unwrap_or(src.len());
        offset = stop;
        Some((start, stop, src[start..stop].trim_end_matches(['\r', '\n'])))
    })
}
fn check_fragment(f: &SourceFragment) -> Result<(), ParseError> {
    if f.content.is_empty()
        || f.content.len() > MAX_FRAGMENT_BYTES
        || f.speaker_label.is_empty()
        || f.speaker_label.len() > 512
    {
        Err(ParseError::Limit)
    } else {
        Ok(())
    }
}
fn parse_eml(src: &str) -> Result<Vec<SourceFragment>, ParseError> {
    let mut from: Option<&str> = None;
    let mut content_type: Option<&str> = None;
    let mut transfer: Option<&str> = None;
    let mut body_start = None;
    for (_, end, line) in iter_lines(src) {
        if line.is_empty() {
            body_start = Some(end);
            break;
        }
        // This deliberately rejects folded headers rather than silently interpreting
        // an incomplete RFC 5322 / MIME grammar as reliable structured data.
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(ParseError::Unsupported("folded email header"));
        }
        let (key, value) = line
            .split_once(':')
            .ok_or(ParseError::Unsupported("email header"))?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(ParseError::Unsupported("email header name"));
        }
        let val = value.trim();
        let key_l = key.to_ascii_lowercase();
        match key_l.as_str() {
            "from" if from.replace(val).is_some() => {
                return Err(ParseError::Unsupported("duplicate From"));
            }
            "content-type" if content_type.replace(val).is_some() => {
                return Err(ParseError::Unsupported("duplicate Content-Type"));
            }
            "content-transfer-encoding" if transfer.replace(val).is_some() => {
                return Err(ParseError::Unsupported("duplicate transfer encoding"));
            }
            "from" => {
                from = Some(val);
            }
            "content-type" => {
                content_type = Some(val);
            }
            "content-transfer-encoding" => {
                transfer = Some(val);
            }
            _ => {}
        }
    }
    let begin = body_start.ok_or(ParseError::Unsupported("email header/body delimiter"))?;
    let label = from.ok_or(ParseError::Unsupported("email From"))?;
    if let Some(ct) = content_type {
        let normalized = ct.to_ascii_lowercase();
        let mut pieces = normalized.split(';').map(str::trim);
        if pieces.next() != Some("text/plain") {
            return Err(ParseError::Unsupported("non-plain-text MIME"));
        }
        for param in pieces {
            if !param.is_empty()
                && param != "charset=utf-8"
                && param != "charset=us-ascii"
                && param != "charset=\"utf-8\""
                && param != "charset=\"us-ascii\""
            {
                return Err(ParseError::Unsupported("MIME parameter"));
            }
        }
    }
    if let Some(encoding) = transfer
        && !["7bit", "8bit"].contains(&encoding.to_ascii_lowercase().as_str())
    {
        return Err(ParseError::Unsupported("encoded email body"));
    }
    let content = &src[begin..];
    if content.trim().is_empty() {
        return Err(ParseError::Empty);
    }
    let f = SourceFragment {
        ordinal: 0,
        byte_start: begin as i32,
        byte_end: src.len() as i32,
        speaker_label: label.to_owned(),
        content: content.to_owned(),
    };
    check_fragment(&f)?;
    Ok(vec![f])
}

fn looks_like_datetime(dt: &str) -> bool {
    // Reject impossible calendar dates: otherwise a quoted line with a malformed
    // timestamp can silently split a message and misattribute its content.
    // The locale-dependent format remains deliberately limited to day/month/year.
    let Some((date, time)) = dt.split_once(',') else {
        return false;
    };
    let mut date_parts = date.trim().split('/');
    let (Some(day), Some(month), Some(year), None) = (
        date_parts.next(),
        date_parts.next(),
        date_parts.next(),
        date_parts.next(),
    ) else {
        return false;
    };
    let mut time_parts = time.trim().split(':');
    let (Some(hour), Some(minute), None) = (
        time_parts.next(),
        time_parts.next(),
        time_parts.next(),
    ) else {
        return false;
    };
    if ![day, month, year, hour, minute].iter().all(|v| {
        !v.is_empty() && v.len() <= 4 && v.bytes().all(|b| b.is_ascii_digit())
    }) || hour.len() != 2 || minute.len() != 2 || year.len() < 2 {
        return false;
    }
    let (Ok(day), Ok(month), Ok(year), Ok(hour), Ok(minute)) = (
        day.parse::<u8>(),
        month.parse::<u8>(),
        year.parse::<u32>(),
        hour.parse::<u8>(),
        minute.parse::<u8>(),
    ) else {
        return false;
    };
    if hour >= 24 || minute >= 60 || !(1..=12).contains(&month) || year == 0 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=days_in_month).contains(&day)
}

fn message_speaker(line: &str) -> Option<&str> {
    // A UTF-8 BOM at the beginning of a Windows export is not a speaker label.
    // Keep original byte offsets: only the header recognition ignores the BOM.
    let line = line.strip_prefix('\u{feff}').unwrap_or(line);
    let rest = if let Some(tail) = line.strip_prefix('[') {
        let (stamp, rest) = tail.split_once("] ")?;
        if !looks_like_datetime(stamp) {
            return None;
        }
        rest
    } else {
        let (stamp, rest) = line.split_once(" - ")?;
        if !looks_like_datetime(stamp) {
            return None;
        }
        rest
    };
    let (speaker, _) = rest.split_once(": ")?;
    let speaker = speaker.trim();
    if speaker.is_empty() || speaker.len() > 512 {
        None
    } else {
        Some(speaker)
    }
}
fn parse_whatsapp(src: &str) -> Result<Vec<SourceFragment>, ParseError> {
    let mut pending: Option<(usize, String)> = None;
    let mut results = Vec::new();
    let mut finish = |start: usize, end: usize, label: String| -> Result<(), ParseError> {
        if results.len() >= MAX_FRAGMENTS || end <= start || end - start > MAX_FRAGMENT_BYTES {
            return Err(ParseError::Limit);
        }
        let f = SourceFragment {
            ordinal: results.len() as i32,
            byte_start: start as i32,
            byte_end: end as i32,
            speaker_label: label,
            content: src[start..end].to_owned(),
        };
        check_fragment(&f)?;
        results.push(f);
        Ok(())
    };
    for (start, _, line) in iter_lines(src) {
        if let Some(speaker) = message_speaker(line) {
            if let Some((old, label)) = pending.replace((start, speaker.to_owned())) {
                finish(old, start, label)?;
            }
        } else if pending.is_none() {
            return Err(ParseError::Unsupported("unattributed WhatsApp preamble"));
        }
    }
    if let Some((start, label)) = pending {
        finish(start, src.len(), label)?;
    }
    if results.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(results)
}

pub fn parse_source(format: SourceFormat, raw: &[u8]) -> Result<ParsedSource, ParseError> {
    if raw.is_empty() || raw.len() > MAX_DOCUMENT_BYTES {
        return Err(ParseError::InvalidSize);
    }
    let src = std::str::from_utf8(raw).map_err(|_| ParseError::NotUtf8)?;
    if src.contains('\0') {
        return Err(ParseError::Unsupported("NUL in text"));
    }
    let fragments = match format {
        SourceFormat::Eml => parse_eml(src)?,
        SourceFormat::WhatsappExport => parse_whatsapp(src)?,
    };
    let sha256 = format!("{:x}", Sha256::digest(raw));
    Ok(ParsedSource {
        sha256,
        byte_length: raw.len() as i64,
        fragments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_whatsapp_multiline_exact_offsets() {
        let s = "[02/11/2026, 17:01] Ana: Olá!\ncontinuation α\n[02/11/2026, 17:02] Beto: Não\n";
        let r = parse_source(SourceFormat::WhatsappExport, s.as_bytes()).unwrap();
        assert_eq!(r.fragments.len(), 2);
        assert_eq!(r.fragments[0].speaker_label, "Ana");
        assert_eq!(r.fragments[1].speaker_label, "Beto");
        for f in &r.fragments {
            assert_eq!(&s[f.byte_start as usize..f.byte_end as usize], f.content);
        }
        assert!(r.fragments[0].content.contains("continuation α"));
    }
    #[test]
    fn alternate_whatsapp_date_format_and_crlf() {
        let s = "02/11/2026, 09:05 - Ana: One\r\n02/11/2026, 09:06 - Beto: Two\r\n";
        assert_eq!(
            parse_source(SourceFormat::WhatsappExport, s.as_bytes())
                .unwrap()
                .fragments
                .len(),
            2
        );
    }
    #[test]
    fn utf8_bom_export_preserves_original_byte_offsets() {
        let src = "\u{feff}[02/11/2026, 17:01] Ana: Olá\n[02/11/2026, 17:02] Beto: Sim\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 2);
        assert_eq!(parsed.fragments[0].byte_start, 0);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        for fragment in parsed.fragments {
            assert_eq!(
                fragment.content.as_bytes(),
                &src.as_bytes()[fragment.byte_start as usize..fragment.byte_end as usize]
            );
        }
    }

    #[test]
    fn impossible_calendar_dates_remain_message_content() {
        let src = "[28/02/2025, 12:00] Ana: Start\n[29/02/2025, 12:01] Fake: quoted line\n[31/04/2025, 12:02] Fake: another quoted line\n[01/03/2025, 12:03] Beto: valid\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 2);
        assert!(parsed.fragments[0].content.contains("29/02/2025"));
        assert!(parsed.fragments[0].content.contains("31/04/2025"));
        assert_eq!(parsed.fragments[1].speaker_label, "Beto");
        assert!(looks_like_datetime("29/02/2024, 12:01"));
        assert!(!looks_like_datetime("29/02/2025, 12:01"));
        assert!(!looks_like_datetime("00/12/2025, 12:01"));
        assert!(!looks_like_datetime("01/13/2025, 12:01"));
    }

    #[test]
    fn email_body_starts_after_headers() {
        let s = "From: <person@example.org>\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\nHello\r\n";
        let r = parse_source(SourceFormat::Eml, s.as_bytes()).unwrap();
        assert_eq!(r.fragments[0].content, "Hello\r\n");
        assert_eq!(&s[r.fragments[0].byte_start as usize..], "Hello\r\n");
    }
    #[test]
    fn rejects_encoded_or_multipart_mime() {
        for h in [
            "Content-Transfer-Encoding: base64",
            "Content-Type: multipart/mixed",
        ] {
            let s = format!("From: x\n{h}\n\nbody");
            assert!(matches!(
                parse_source(SourceFormat::Eml, s.as_bytes()),
                Err(ParseError::Unsupported(_))
            ));
        }
    }
    #[test]
    fn rejects_unattributed_or_invalid_input() {
        assert!(parse_source(SourceFormat::WhatsappExport, b"system message\n").is_err());
        assert_eq!(
            parse_source(SourceFormat::Eml, &[0xff]),
            Err(ParseError::NotUtf8)
        );
        assert_eq!(
            parse_source(SourceFormat::Eml, b""),
            Err(ParseError::InvalidSize)
        );
    }
    #[test]
    fn rejects_large_fragment_and_non_ascii_identity_is_preserved() {
        let large = format!("From: Test\n\n{}", "x".repeat(MAX_FRAGMENT_BYTES + 1));
        assert_eq!(
            parse_source(SourceFormat::Eml, large.as_bytes()),
            Err(ParseError::Limit)
        );
        let r = parse_source(
            SourceFormat::WhatsappExport,
            "[02/11/2026, 08:11] João: está ok".as_bytes(),
        )
        .unwrap();
        assert_eq!(r.fragments[0].speaker_label, "João");
    }
    #[test]
    fn digest_is_over_original_bytes() {
        let a = parse_source(SourceFormat::Eml, b"From: x\n\none").unwrap();
        let b = parse_source(SourceFormat::Eml, b"From: x\r\n\r\none").unwrap();
        assert_ne!(a.sha256, b.sha256);
    }
}

#[cfg(test)]
mod adversarial {
    use super::*;
    #[test]
    fn seeded_4096_mutations_never_panic_or_break_byte_bounds() {
        let mut state: u64 = 0x9e3779b97f4a7c15;
        let template = b"[02/11/2026, 10:15] Ana: sample\n[02/11/2026, 10:16] Bob: example\n";
        for case in 0..4096 {
            let mut bytes = if case % 2 == 0 {
                template.to_vec()
            } else {
                Vec::new()
            };
            for _ in 0..(case % 95) {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                if bytes.is_empty() || state & 3 == 0 {
                    bytes.push((state & 0xff) as u8);
                } else {
                    let idx = state as usize % bytes.len();
                    bytes[idx] = (state >> 16) as u8;
                }
            }
            for format in [SourceFormat::Eml, SourceFormat::WhatsappExport] {
                let result = std::panic::catch_unwind(|| parse_source(format, &bytes));
                let parsed = result.expect("parser must never panic on arbitrary byte strings");
                if let Ok(parsed) = parsed {
                    assert_eq!(parsed.byte_length, bytes.len() as i64);
                    for (ordinal, f) in parsed.fragments.iter().enumerate() {
                        assert_eq!(f.ordinal as usize, ordinal);
                        assert!(f.byte_start >= 0 && f.byte_end > f.byte_start);
                        assert!((f.byte_end as usize) <= bytes.len());
                        assert_eq!(
                            f.content.as_bytes(),
                            &bytes[f.byte_start as usize..f.byte_end as usize]
                        );
                    }
                }
            }
        }
    }
}
