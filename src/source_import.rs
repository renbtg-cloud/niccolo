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
fn unsafe_identity_character(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{061c}'
                | '\u{200e}'
                | '\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        )
}

fn valid_speaker_label(label: &str) -> bool {
    !label.is_empty() && label.len() <= 512 && !label.chars().any(unsafe_identity_character)
}

fn check_fragment(f: &SourceFragment) -> Result<(), ParseError> {
    if f.content.is_empty() || f.content.len() > MAX_FRAGMENT_BYTES {
        Err(ParseError::Limit)
    } else if !valid_speaker_label(&f.speaker_label) {
        Err(ParseError::Unsupported("unsafe speaker label"))
    } else {
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct EmailHeader {
    speaker_label: String,
    body_start: usize,
}

fn invalid_email_header<T>(nested: bool, reason: &'static str) -> Result<Option<T>, ParseError> {
    if nested {
        Ok(None)
    } else {
        Err(ParseError::Unsupported(reason))
    }
}

fn parse_email_header(
    src: &str,
    start: usize,
    nested: bool,
) -> Result<Option<EmailHeader>, ParseError> {
    let mut from: Option<&str> = None;
    let mut content_type: Option<String> = None;
    let mut transfer: Option<String> = None;
    let mut body_start = None;
    let mut has_date = false;
    let mut has_subject = false;
    let mut previous_header: Option<String> = None;

    for (relative_start, relative_end, line) in iter_lines(&src[start..]) {
        let line = if start == 0 && relative_start == 0 {
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };
        if line.is_empty() {
            body_start = Some(start + relative_end);
            break;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // Unfold only the MIME values that we subsequently validate.
            // Never unfold a sender identity without RFC 5322 mailbox parsing.
            let extra = line.trim();
            match previous_header.as_deref() {
                Some("from") | None => {
                    return invalid_email_header(nested, "folded email identity header");
                }
                Some("content-type") => {
                    let Some(value) = content_type.as_mut() else {
                        return invalid_email_header(nested, "MIME continuation");
                    };
                    if value.len() + 1 + extra.len() > 4096 {
                        return invalid_email_header(nested, "MIME header too long");
                    }
                    value.push(' ');
                    value.push_str(extra);
                }
                Some("content-transfer-encoding") => {
                    let Some(value) = transfer.as_mut() else {
                        return invalid_email_header(nested, "transfer continuation");
                    };
                    if value.len() + 1 + extra.len() > 4096 {
                        return invalid_email_header(nested, "transfer header too long");
                    }
                    value.push(' ');
                    value.push_str(extra);
                }
                Some(_) => {}
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return invalid_email_header(nested, "email header");
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return invalid_email_header(nested, "email header name");
        }
        let value = value.trim();
        let normalized_key = key.to_ascii_lowercase();
        previous_header = Some(normalized_key.clone());
        match normalized_key.as_str() {
            "from" if from.replace(value).is_some() => {
                return invalid_email_header(nested, "duplicate From");
            }
            "content-type" if content_type.replace(value.to_owned()).is_some() => {
                return invalid_email_header(nested, "duplicate Content-Type");
            }
            "content-transfer-encoding" if transfer.replace(value.to_owned()).is_some() => {
                return invalid_email_header(nested, "duplicate transfer encoding");
            }
            "from" => from = Some(value),
            "content-type" => content_type = Some(value.to_owned()),
            "content-transfer-encoding" => transfer = Some(value.to_owned()),
            "date" => has_date = true,
            "subject" => has_subject = true,
            _ => {}
        }
    }

    let Some(begin) = body_start else {
        return invalid_email_header(nested, "email header/body delimiter");
    };
    let Some(label) = from else {
        return invalid_email_header(nested, "email From");
    };
    if nested && (!has_date || !has_subject) {
        return Ok(None);
    }

    if let Some(ct) = content_type {
        let normalized = ct.to_ascii_lowercase();
        let mut pieces = normalized.split(';').map(str::trim);
        if pieces.next() != Some("text/plain") {
            return invalid_email_header(nested, "non-plain-text MIME");
        }
        let mut seen_params = std::collections::BTreeSet::new();
        for param in pieces {
            if param.is_empty() {
                continue;
            }
            let Some((key, value)) = param.split_once('=') else {
                return invalid_email_header(nested, "MIME parameter");
            };
            let key = key.trim();
            let raw_value = value.trim();
            let value = if raw_value.starts_with('"') || raw_value.ends_with('"') {
                if raw_value.len() < 2 || !raw_value.starts_with('"') || !raw_value.ends_with('"') {
                    return invalid_email_header(nested, "MIME parameter quoting");
                }
                &raw_value[1..raw_value.len() - 1]
            } else {
                raw_value
            };
            if value.contains('"') || key.is_empty() || value.is_empty() || !seen_params.insert(key)
            {
                return invalid_email_header(
                    nested,
                    if seen_params.contains(key) {
                        "duplicate MIME parameter"
                    } else {
                        "MIME parameter"
                    },
                );
            }
            let allowed = match key {
                "charset" => matches!(value, "utf-8" | "us-ascii"),
                // RFC 3676 flowed text is still plain text. We preserve the
                // uploaded bytes exactly rather than silently reflowing them.
                "format" => value == "flowed",
                "delsp" => matches!(value, "yes" | "no"),
                _ => false,
            };
            if !allowed {
                return invalid_email_header(nested, "MIME parameter");
            }
        }
    }
    if let Some(encoding) = transfer
        && !["7bit", "8bit"].contains(&encoding.to_ascii_lowercase().as_str())
    {
        return invalid_email_header(nested, "encoded email body");
    }

    Ok(Some(EmailHeader {
        speaker_label: label.to_owned(),
        body_start: begin,
    }))
}

fn email_thread_boundary(src: &str, start: usize) -> bool {
    start > 0 && (src[..start].ends_with("\n\n") || src[..start].ends_with("\r\n\r\n"))
}

fn next_embedded_email_header(
    src: &str,
    body_start: usize,
) -> Result<Option<(usize, EmailHeader)>, ParseError> {
    for (relative_start, _, line) in iter_lines(&src[body_start..]) {
        let start = body_start + relative_start;
        if start == body_start || !email_thread_boundary(src, start) {
            continue;
        }
        let Some((key, _)) = line.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("from") {
            continue;
        }
        if let Some(header) = parse_email_header(src, start, true)? {
            return Ok(Some((start, header)));
        }
    }
    Ok(None)
}

fn push_email_fragment(
    fragments: &mut Vec<SourceFragment>,
    src: &str,
    start: usize,
    end: usize,
    speaker_label: String,
) -> Result<(), ParseError> {
    if fragments.len() >= MAX_FRAGMENTS || end <= start || end - start > MAX_FRAGMENT_BYTES {
        return Err(ParseError::Limit);
    }
    if src[start..end].trim().is_empty() {
        return Err(ParseError::Empty);
    }
    let fragment = SourceFragment {
        ordinal: fragments.len() as i32,
        byte_start: start as i32,
        byte_end: end as i32,
        speaker_label,
        content: src[start..end].to_owned(),
    };
    check_fragment(&fragment)?;
    fragments.push(fragment);
    Ok(())
}

fn parse_eml(src: &str) -> Result<Vec<SourceFragment>, ParseError> {
    let first =
        parse_email_header(src, 0, false)?.ok_or(ParseError::Unsupported("email header"))?;
    let mut speaker = first.speaker_label;
    let mut body_start = first.body_start;
    let mut fragments = Vec::new();

    loop {
        let next = next_embedded_email_header(src, body_start)?;
        let body_end = next.as_ref().map_or(src.len(), |(start, _)| *start);
        push_email_fragment(&mut fragments, src, body_start, body_end, speaker)?;
        let Some((_, header)) = next else {
            break;
        };
        speaker = header.speaker_label;
        body_start = header.body_start;
    }

    Ok(fragments)
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
    // Some WhatsApp exports include seconds and/or an AM/PM suffix.
    // Accept only those explicit forms; never guess the date locale.
    let (clock, meridiem) = match time.trim().rsplit_once(' ') {
        Some((clock, suffix)) if suffix.eq_ignore_ascii_case("AM") => (clock, Some("AM")),
        Some((clock, suffix)) if suffix.eq_ignore_ascii_case("PM") => (clock, Some("PM")),
        Some(_) => return false,
        None => (time.trim(), None),
    };
    let mut time_parts = clock.split(':');
    let (Some(hour), Some(minute)) = (time_parts.next(), time_parts.next()) else {
        return false;
    };
    let second = time_parts.next();
    // WhatsApp date fields are one or two digits; years are two or four.
    // Accepting 001/002/026 lets a plausible-looking quoted line fabricate
    // a new speaker boundary despite not being a supported export timestamp.
    if time_parts.next().is_some()
        || !(1..=2).contains(&day.len())
        || !(1..=2).contains(&month.len())
        || (year.len() != 2 && year.len() != 4)
        || ![day, month, year, hour, minute]
            .iter()
            .all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        || hour.is_empty()
        || hour.len() > 2
        || minute.len() != 2
        || second.is_some_and(|s| s.len() != 2 || !s.bytes().all(|b| b.is_ascii_digit()))
        || year.len() < 2
    {
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
    if second.is_some_and(|s| s.parse::<u8>().map_or(true, |value| value >= 60))
        || minute >= 60
        || !(1..=12).contains(&month)
        || year == 0
        || match meridiem {
            Some(_) => !(1..=12).contains(&hour),
            None => hour >= 24,
        }
    {
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

fn timestamped_whatsapp_body(line: &str) -> Option<&str> {
    if let Some(tail) = line.strip_prefix('[') {
        let (stamp, rest) = tail.split_once("] ")?;
        looks_like_datetime(stamp).then_some(rest)
    } else {
        let (stamp, rest) = line.split_once(" - ")?;
        looks_like_datetime(stamp).then_some(rest)
    }
}

fn message_speaker(line: &str) -> Option<&str> {
    let rest = timestamped_whatsapp_body(line)?;
    let (speaker, _) = rest.split_once(": ")?;
    let speaker = speaker.trim();
    valid_speaker_label(speaker).then_some(speaker)
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
        // BOM is legitimate only at byte offset zero. Stripping one from
        // later lines can forge an apparent message boundary in quoted text.
        let header = if start == 0 {
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };
        // WhatsApp includes timestamped system notices (member changes,
        // encryption banners, etc.) without a "speaker: " prefix. Do not
        // silently attribute such notices to the preceding human speaker.
        let timestamped_body = timestamped_whatsapp_body(header);
        let attribution = if let Some(speaker) = message_speaker(header) {
            Some(speaker.to_owned())
        } else if let Some(body) = timestamped_body {
            // A timestamped line with the human-message delimiter but an
            // invalid speaker label is ambiguous. Never downgrade it to a
            // system event, which would hide an attribution-spoofing attempt.
            if body.contains(": ") {
                return Err(ParseError::Unsupported(
                    "unsafe or ambiguous WhatsApp speaker label",
                ));
            }
            Some("[WhatsApp system event — unverified]".to_owned())
        } else {
            None
        };
        if let Some(speaker) = attribution {
            if let Some((old, label)) = pending.replace((start, speaker)) {
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
    if src
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
    {
        return Err(ParseError::Unsupported("control character in text"));
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
    fn timestamped_system_notice_does_not_impersonate_previous_speaker() {
        let src = "[02/11/2026, 10:00] Ana: I will join.\n[02/11/2026, 10:01] Ana changed the group description\n[02/11/2026, 10:02] Beto: Thanks.\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 3);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        assert_eq!(
            parsed.fragments[1].speaker_label,
            "[WhatsApp system event — unverified]"
        );
        assert!(
            parsed.fragments[1]
                .content
                .contains("changed the group description")
        );
        assert_eq!(parsed.fragments[2].speaker_label, "Beto");
        for fragment in parsed.fragments {
            assert_eq!(
                fragment.content.as_bytes(),
                &src.as_bytes()[fragment.byte_start as usize..fragment.byte_end as usize]
            );
        }
    }

    #[test]
    fn system_notice_before_first_message_is_preserved_without_human_attribution() {
        let src = "02/11/2026, 10:00 - Messages and calls are end-to-end encrypted.\r\n02/11/2026, 10:01 - Ana: Good morning.\r\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 2);
        assert_eq!(
            parsed.fragments[0].speaker_label,
            "[WhatsApp system event — unverified]"
        );
        assert_eq!(parsed.fragments[1].speaker_label, "Ana");
        for fragment in parsed.fragments {
            assert_eq!(
                fragment.content.as_bytes(),
                &src.as_bytes()[fragment.byte_start as usize..fragment.byte_end as usize]
            );
        }
    }

    #[test]
    fn lowercase_meridiem_whatsapp_timestamps_are_supported_without_locale_guessing() {
        let src = "[02/11/2026, 9:05 pm] Ana: Evening.\n[02/11/2026, 9:06 PM] Beto: Same clock.\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 2);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        assert_eq!(parsed.fragments[1].speaker_label, "Beto");
        assert!(looks_like_datetime("02/11/2026, 9:05 am"));
        assert!(!looks_like_datetime("02/11/2026, 13:05 pm"));
    }

    #[test]
    fn single_digit_hours_in_local_whatsapp_exports_remain_attributed() {
        let src = "[02/11/2026, 9:05] Ana: Morning.\n[02/11/2026, 10:05] Beto: Hello.\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 2);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        assert_eq!(parsed.fragments[1].speaker_label, "Beto");
        assert!(!looks_like_datetime("02/11/2026, 25:05"));
    }

    #[test]
    fn whatsapp_seconds_and_twelve_hour_clock_preserve_speaker_boundaries() {
        let src = "[02/11/2026, 9:05:31 AM] Ana: Morning.\n[02/11/2026, 12:06:59 PM] Beto: Afternoon.\n[02/11/2026, 13:07:05] Carla: Hello.\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 3);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        assert_eq!(parsed.fragments[1].speaker_label, "Beto");
        assert_eq!(parsed.fragments[2].speaker_label, "Carla");
        for fragment in parsed.fragments {
            assert_eq!(
                fragment.content.as_bytes(),
                &src.as_bytes()[fragment.byte_start as usize..fragment.byte_end as usize]
            );
        }
        for invalid in [
            "02/11/2026, 13:05 PM",
            "02/11/2026, 00:05 AM",
            "02/11/2026, 10:05:60",
            "02/11/2026, 10:05:1",
            "02/11/2026, 10:05:01:02",
            "02/11/2026, 10:05 XM",
        ] {
            assert!(
                !looks_like_datetime(invalid),
                "invalid timestamp accepted: {invalid}"
            );
        }
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
    fn embedded_bom_cannot_create_another_whatsapp_speaker() {
        let src = "[02/11/2026, 17:01] Ana: Hi\n\u{feff}[02/11/2026, 17:02] Fake: quoted\n[02/11/2026, 17:03] Beto: reply\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 2);
        assert!(parsed.fragments[0].content.contains("Fake: quoted"));
        assert_eq!(parsed.fragments[1].speaker_label, "Beto");
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
    fn directional_control_characters_cannot_spoof_source_speaker_labels() {
        for source in [
            "From: Alice \u{202e}<mallory@example.org>\nSubject: test\n\nbody",
            "From: Alice \u{2067}<mallory@example.org>\nSubject: test\n\nbody",
        ] {
            assert!(matches!(
                parse_source(SourceFormat::Eml, source.as_bytes()),
                Err(ParseError::Unsupported("unsafe speaker label"))
            ));
        }
        for source in [
            "[02/11/2026, 17:01] Alice\u{202e}: hello\n",
            "[02/11/2026, 17:01] Alice\u{2067}: hello\n",
        ] {
            assert!(parse_source(SourceFormat::WhatsappExport, source.as_bytes()).is_err());
        }
        let ordinary = "[02/11/2026, 17:01] João: olá\n";
        assert_eq!(
            parse_source(SourceFormat::WhatsappExport, ordinary.as_bytes())
                .unwrap()
                .fragments[0]
                .speaker_label,
            "João"
        );
    }

    #[test]
    fn canonical_full_email_thread_preserves_sender_boundaries() {
        let raw = include_bytes!("../fixtures/law-firm-mvp/sources/02_full_thread.eml");
        let parsed = parse_source(SourceFormat::Eml, raw).unwrap();
        assert_eq!(parsed.fragments.len(), 3);
        assert_eq!(
            parsed
                .fragments
                .iter()
                .map(|fragment| fragment.speaker_label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "julian.mercer@aurelius.example",
                "adrian.kessler@aurelius.example",
                "adrian.kessler@aurelius.example",
            ]
        );
        assert!(
            parsed.fragments[0]
                .content
                .contains("committee meets at noon")
        );
        assert!(
            parsed.fragments[1]
                .content
                .contains("Push this through before the committee sees it.")
        );
        assert!(
            parsed.fragments[2]
                .content
                .contains("I am not authorizing the settlement or bonus.")
        );
        for fragment in parsed.fragments {
            assert_eq!(
                fragment.content.as_bytes(),
                &raw[fragment.byte_start as usize..fragment.byte_end as usize]
            );
        }
    }

    #[test]
    fn incomplete_from_like_body_text_is_not_promoted_to_an_email_boundary() {
        let src = "From: root@example.org\nSubject: Actual message\n\nIntro\n\nFrom: quoted@example.org\nThis is prose, not a complete nested email header.\n";
        let parsed = parse_source(SourceFormat::Eml, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].speaker_label, "root@example.org");
        assert!(
            parsed.fragments[0]
                .content
                .contains("From: quoted@example.org")
        );
    }

    #[test]
    fn folded_email_subject_and_ignored_headers_preserve_exact_body_offsets() {
        let src = "From: ana@example.org\r\nSubject: A complicated\r\n follow-up subject\r\nX-Trace: first\r\n second\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nOriginal body\r\n";
        let parsed = parse_source(SourceFormat::Eml, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].speaker_label, "ana@example.org");
        assert_eq!(parsed.fragments[0].content, "Original body\r\n");
        let f = &parsed.fragments[0];
        assert_eq!(
            f.content.as_bytes(),
            &src.as_bytes()[f.byte_start as usize..f.byte_end as usize]
        );
    }

    #[test]
    fn folded_plain_text_mime_headers_are_validated_after_unfolding() {
        let src = "From: ana@example.org\r\nContent-Type: text/plain;\r\n charset=UTF-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\nOlá\r\n";
        let parsed = parse_source(SourceFormat::Eml, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].speaker_label, "ana@example.org");
        assert_eq!(parsed.fragments[0].content, "Olá\r\n");
    }

    #[test]
    fn folded_email_identity_and_unsupported_mime_remain_rejected() {
        for src in [
            "From: ana@example.org\r\n attacker@example.org\r\n\r\nBody",
            "From: ana@example.org\r\nContent-Type: text/html;\r\n charset=utf-8\r\n\r\nBody",
            "From: ana@example.org\r\nContent-Transfer-Encoding: base64\r\n malicious\r\n\r\nBody",
        ] {
            assert!(matches!(
                parse_source(SourceFormat::Eml, src.as_bytes()),
                Err(ParseError::Unsupported(_))
            ));
        }
    }

    #[test]
    fn email_body_starts_after_headers() {
        let s = "From: <person@example.org>\r\nContent-Type: text/plain; charset=UTF-8\r\n\r\nHello\r\n";
        let r = parse_source(SourceFormat::Eml, s.as_bytes()).unwrap();
        assert_eq!(r.fragments[0].content, "Hello\r\n");
        assert_eq!(&s[r.fragments[0].byte_start as usize..], "Hello\r\n");
    }
    #[test]
    fn utf8_bom_email_preserves_original_body_offsets() {
        let src = "\u{feff}From: Ana <ana@example.org>\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nOlá!\r\n";
        let parsed = parse_source(SourceFormat::Eml, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana <ana@example.org>");
        let fragment = &parsed.fragments[0];
        assert_eq!(
            fragment.content.as_bytes(),
            &src.as_bytes()[fragment.byte_start as usize..fragment.byte_end as usize]
        );
        assert_eq!(fragment.content, "Olá!\r\n");
    }

    #[test]
    fn accepts_plain_text_format_flowed_without_rewriting_source_bytes() {
        let src = "From: Ana <ana@example.org>\r\nContent-Type: text/plain; charset=UTF-8; format=flowed; delsp=yes\r\nContent-Transfer-Encoding: 8bit\r\n\r\nA flowed line \r\ncontinues here.\r\n";
        let parsed = parse_source(SourceFormat::Eml, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        let fragment = &parsed.fragments[0];
        assert_eq!(fragment.speaker_label, "Ana <ana@example.org>");
        assert_eq!(
            fragment.content.as_bytes(),
            &src.as_bytes()[fragment.byte_start as usize..fragment.byte_end as usize]
        );
        assert!(
            fragment
                .content
                .contains("A flowed line \r\ncontinues here.")
        );
    }

    #[test]
    fn accepts_quoted_flowed_mime_parameters_but_not_unknown_plain_text_parameters() {
        let accepted = "From: x@example.org\nContent-Type: text/plain; charset=\"UTF-8\"; format=\"flowed\"; delsp=\"no\"\n\nbody";
        assert!(parse_source(SourceFormat::Eml, accepted.as_bytes()).is_ok());
        let rejected =
            "From: x@example.org\nContent-Type: text/plain; charset=UTF-8; x-unknown=yes\n\nbody";
        assert!(matches!(
            parse_source(SourceFormat::Eml, rejected.as_bytes()),
            Err(ParseError::Unsupported("MIME parameter"))
        ));
    }

    #[test]
    fn accepts_optional_whitespace_around_plain_text_mime_parameter_values() {
        let src = "From: x@example.org\nContent-Type: text/plain; charset = \"UTF-8\"; format = flowed; delsp = yes\n\nbody";
        let parsed = parse_source(SourceFormat::Eml, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].content, "body");
    }

    #[test]
    fn rejects_malformed_or_nested_quotes_in_mime_parameter_values() {
        for content_type in [
            "text/plain; charset=\"UTF-8",
            "text/plain; charset=UTF-8\"",
            "text/plain; charset=\"\"UTF-8\"\"",
        ] {
            let src = format!("From: x@example.org\nContent-Type: {content_type}\n\nbody");
            assert!(matches!(
                parse_source(SourceFormat::Eml, src.as_bytes()),
                Err(ParseError::Unsupported("MIME parameter quoting"))
                    | Err(ParseError::Unsupported("MIME parameter"))
            ));
        }
    }

    #[test]
    fn rejects_duplicate_plain_text_mime_parameters() {
        for content_type in [
            "text/plain; charset=UTF-8; charset=US-ASCII",
            "text/plain; format=flowed; format=\"flowed\"",
            "text/plain; format=flowed; delsp=yes; delsp=no",
        ] {
            let src = format!("From: x@example.org\nContent-Type: {content_type}\n\nbody");
            assert!(matches!(
                parse_source(SourceFormat::Eml, src.as_bytes()),
                Err(ParseError::Unsupported("duplicate MIME parameter"))
            ));
        }
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
    fn rejects_terminal_control_characters_but_preserves_tabs_and_line_breaks() {
        let dangerous = b"From: x@example.org\n\nhello\x1b[31mred";
        assert_eq!(
            parse_source(SourceFormat::Eml, dangerous),
            Err(ParseError::Unsupported("control character in text"))
        );
        let ordinary = b"From: x@example.org\n\nhello\tworld\r\n";
        assert!(parse_source(SourceFormat::Eml, ordinary).is_ok());
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
