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
    let mut content_type: Option<&str> = None;
    let mut transfer: Option<&str> = None;
    let mut body_start = None;
    let mut has_date = false;
    let mut has_subject = false;

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
            return invalid_email_header(nested, "folded email header");
        }
        let Some((key, value)) = line.split_once(':') else {
            return invalid_email_header(nested, "email header");
        };
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return invalid_email_header(nested, "email header name");
        }
        let value = value.trim();
        match key.to_ascii_lowercase().as_str() {
            "from" if from.replace(value).is_some() => {
                return invalid_email_header(nested, "duplicate From");
            }
            "content-type" if content_type.replace(value).is_some() => {
                return invalid_email_header(nested, "duplicate Content-Type");
            }
            "content-transfer-encoding" if transfer.replace(value).is_some() => {
                return invalid_email_header(nested, "duplicate transfer encoding");
            }
            "from" => from = Some(value),
            "content-type" => content_type = Some(value),
            "content-transfer-encoding" => transfer = Some(value),
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
        for param in pieces {
            if !param.is_empty()
                && param != "charset=utf-8"
                && param != "charset=us-ascii"
                && param != "charset=\"utf-8\""
                && param != "charset=\"us-ascii\""
            {
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

fn ascii_number(raw: &str, max_len: usize) -> Option<u32> {
    if raw.is_empty() || raw.len() > max_len || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

fn normalized_year(raw: &str) -> Option<u32> {
    match raw.len() {
        2 => ascii_number(raw, 2).map(|year| 2000 + year),
        4 => ascii_number(raw, 4).filter(|year| *year > 0),
        _ => None,
    }
}

fn valid_calendar_date(year: u32, month: u32, day: u32) -> bool {
    if !(1..=12).contains(&month) || day == 0 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    day <= days_in_month
}

fn valid_whatsapp_date(raw: &str) -> bool {
    let separator = ['/', '.', '-']
        .into_iter()
        .find(|separator| raw.contains(*separator));
    let Some(separator) = separator else {
        return false;
    };
    if raw.chars().filter(|ch| *ch == separator).count() != 2 {
        return false;
    }
    let parts: Vec<&str> = raw.split(separator).collect();
    if parts.len() != 3 || parts[0].len() > 4 || parts[1].len() > 2 || parts[2].len() > 4 {
        return false;
    }
    let Some(first) = ascii_number(parts[0], 4) else {
        return false;
    };
    let Some(second) = ascii_number(parts[1], 2) else {
        return false;
    };
    let Some(third) = ascii_number(parts[2], 4) else {
        return false;
    };

    if parts[0].len() == 4 {
        return first > 0 && valid_calendar_date(first, second, third);
    }
    let Some(year) = normalized_year(parts[2]) else {
        return false;
    };
    valid_calendar_date(year, second, first) || valid_calendar_date(year, first, second)
}

fn valid_whatsapp_time(raw: &str, meridiem: Option<&str>) -> bool {
    let parts: Vec<&str> = raw.split(':').collect();
    if !(2..=3).contains(&parts.len())
        || parts[0].is_empty()
        || parts[0].len() > 2
        || parts[1].len() != 2
        || (parts.len() == 3 && parts[2].len() != 2)
    {
        return false;
    }
    let Some(hour) = ascii_number(parts[0], 2) else {
        return false;
    };
    let Some(minute) = ascii_number(parts[1], 2) else {
        return false;
    };
    let second = if parts.len() == 3 {
        let Some(second) = ascii_number(parts[2], 2) else {
            return false;
        };
        second
    } else {
        0
    };
    if minute >= 60 || second >= 60 {
        return false;
    }
    match meridiem {
        Some(marker) if marker.eq_ignore_ascii_case("am") || marker.eq_ignore_ascii_case("pm") => {
            (1..=12).contains(&hour)
        }
        Some(_) => false,
        None => hour < 24,
    }
}

fn looks_like_datetime(dt: &str) -> bool {
    // WhatsApp export timestamps vary by phone and locale. We only decide
    // whether a line is a plausible message boundary; the timestamp is never
    // normalized into evidence or used to infer chronology here.
    let dt = dt
        .trim_start_matches(|ch| matches!(ch, '\u{200e}' | '\u{200f}'))
        .trim();
    let mut pieces = dt.split_whitespace();
    let Some(date) = pieces.next() else {
        return false;
    };
    let Some(time) = pieces.next() else {
        return false;
    };
    let meridiem = pieces.next();
    if pieces.next().is_some() {
        return false;
    }
    let date = date.strip_suffix(',').unwrap_or(date);
    valid_whatsapp_date(date) && valid_whatsapp_time(time, meridiem)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WhatsappLine<'a> {
    Attributed(&'a str),
    System,
}

fn whatsapp_line(line: &str) -> Option<WhatsappLine<'_>> {
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
    let Some((speaker, _)) = rest.split_once(": ") else {
        return Some(WhatsappLine::System);
    };
    let speaker = speaker.trim();
    if speaker.is_empty() || speaker.len() > 512 {
        Some(WhatsappLine::System)
    } else {
        Some(WhatsappLine::Attributed(speaker))
    }
}

fn parse_whatsapp(src: &str) -> Result<Vec<SourceFragment>, ParseError> {
    let mut pending: Option<(usize, String)> = None;
    let mut saw_timestamped_boundary = false;
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
        // BOM is legitimate only at byte offset zero. Directional marks, unlike
        // BOMs, are emitted by some WhatsApp exports and are ignored only for
        // boundary recognition; the original fragment bytes remain untouched.
        let header = if start == 0 {
            line.strip_prefix('\u{feff}').unwrap_or(line)
        } else {
            line
        };
        let header = header.trim_start_matches(|ch| matches!(ch, '\u{200e}' | '\u{200f}'));
        match whatsapp_line(header) {
            Some(WhatsappLine::Attributed(speaker)) => {
                if let Some((old, label)) = pending.replace((start, speaker.to_owned())) {
                    finish(old, start, label)?;
                }
                saw_timestamped_boundary = true;
            }
            Some(WhatsappLine::System) => {
                if let Some((old, label)) = pending.take() {
                    finish(old, start, label)?;
                }
                saw_timestamped_boundary = true;
            }
            None if pending.is_none() && !saw_timestamped_boundary => {
                return Err(ParseError::Unsupported("unattributed WhatsApp preamble"));
            }
            None => {}
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
    fn common_whatsapp_timestamp_variants_preserve_message_boundaries() {
        let src = concat!(
            "30/04/2022 08:31 - Ana: Brazilian no-comma format\n",
            "3/15/24, 2:30 PM - Bob: US 12-hour format\n",
            "[15.03.24, 10:30] Carla: dotted iOS-style format\n",
            "[2024/07/09, 08:01:49] Davi: year-first with seconds\n",
            "[2026-03-02, 6:39:22 AM] Eva: year-first 12-hour\n",
        );
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 5);
        assert_eq!(
            parsed
                .fragments
                .iter()
                .map(|fragment| fragment.speaker_label.as_str())
                .collect::<Vec<_>>(),
            vec!["Ana", "Bob", "Carla", "Davi", "Eva"]
        );
        for fragment in parsed.fragments {
            assert_eq!(
                fragment.content.as_bytes(),
                &src.as_bytes()[fragment.byte_start as usize..fragment.byte_end as usize]
            );
        }
    }

    #[test]
    fn flexible_whatsapp_timestamp_detection_still_rejects_impossible_boundaries() {
        for invalid in [
            "31/04/2025 12:01",
            "13/13/2025, 12:01",
            "2025-02-29, 12:01",
            "3/15/24, 13:30 PM",
            "15.03.24, 24:00",
            "15.03.24, 10:30:60",
        ] {
            assert!(
                !looks_like_datetime(invalid),
                "accepted invalid timestamp {invalid}"
            );
        }
        for valid in [
            "30/04/2022 08:31",
            "3/15/24, 2:30 PM",
            "15.03.24, 10:30",
            "2024/07/09, 08:01:49",
            "2026-03-02, 6:39:22 AM",
        ] {
            assert!(
                looks_like_datetime(valid),
                "rejected valid timestamp {valid}"
            );
        }
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
    fn timestamped_system_notice_before_first_message_is_not_attributed() {
        let src = concat!(
            "30/04/2022 08:30 - Messages and calls are end-to-end encrypted.\n",
            "30/04/2022 08:31 - Ana: Oi\n",
        );
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        assert_eq!(
            parsed.fragments[0].byte_start as usize,
            src.find("30/04/2022 08:31 - Ana").unwrap()
        );
        assert!(!parsed.fragments[0].content.contains("end-to-end encrypted"));
    }

    #[test]
    fn system_only_whatsapp_export_does_not_create_synthetic_speaker_evidence() {
        let src = concat!(
            "30/04/2022 08:30 - Messages and calls are end-to-end encrypted.\n",
            "30/04/2022 08:31 - You changed the group description\n",
        );
        assert_eq!(
            parse_source(SourceFormat::WhatsappExport, src.as_bytes()),
            Err(ParseError::Empty)
        );
    }

    #[test]
    fn timestamped_system_notice_between_messages_closes_prior_attribution() {
        let src = concat!(
            "30/04/2022 08:31 - Ana: Before\n",
            "continuation from Ana\n",
            "30/04/2022 08:32 - Messages and calls are end-to-end encrypted.\n",
            "system continuation without participant attribution\n",
            "30/04/2022 08:33 - Beto: After\n",
        );
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 2);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        assert_eq!(parsed.fragments[1].speaker_label, "Beto");
        assert!(
            parsed.fragments[0]
                .content
                .contains("continuation from Ana")
        );
        assert!(!parsed.fragments[0].content.contains("end-to-end encrypted"));
        assert!(!parsed.fragments[1].content.contains("system continuation"));
    }

    #[test]
    fn directional_mark_before_real_header_does_not_change_raw_offsets() {
        let src = "\u{200e}[4/29/26, 8:11:15 AM] Ana: Hello\n";
        let parsed = parse_source(SourceFormat::WhatsappExport, src.as_bytes()).unwrap();
        assert_eq!(parsed.fragments.len(), 1);
        assert_eq!(parsed.fragments[0].speaker_label, "Ana");
        assert_eq!(parsed.fragments[0].byte_start, 0);
        assert_eq!(parsed.fragments[0].content, src);
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
        assert!(!looks_like_datetime("13/13/2025, 12:01"));
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
