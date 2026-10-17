//! A tiny RFC 5545 calendar invite for one planned date.

pub struct Event<'a> {
    pub uid: String,
    /// Unix seconds.
    pub start: i64,
    pub minutes: i64,
    pub summary: &'a str,
    pub location: &'a str,
    pub description: &'a str,
    /// Unix seconds, for DTSTAMP.
    pub now: i64,
}

pub fn render(e: &Event) -> String {
    let lines = [
        "BEGIN:VCALENDAR".to_string(),
        "VERSION:2.0".into(),
        "PRODID:-//joy//date jar//EN".into(),
        "CALSCALE:GREGORIAN".into(),
        "METHOD:PUBLISH".into(),
        "BEGIN:VEVENT".into(),
        format!("UID:{}", e.uid),
        format!("DTSTAMP:{}", utc(e.now)),
        format!("DTSTART:{}", utc(e.start)),
        format!("DTEND:{}", utc(e.start + e.minutes * 60)),
        format!("SUMMARY:{}", escape(e.summary)),
        format!("LOCATION:{}", escape(e.location)),
        format!("DESCRIPTION:{}", escape(e.description)),
        "BEGIN:VALARM".into(),
        "ACTION:DISPLAY".into(),
        "DESCRIPTION:Date soon".into(),
        "TRIGGER:-PT2H".into(),
        "END:VALARM".into(),
        "END:VEVENT".into(),
        "END:VCALENDAR".into(),
    ];
    lines.iter().map(|l| fold(l)).collect::<Vec<_>>().join("\r\n") + "\r\n"
}

/// How long to block out for each duration bucket.
pub fn minutes_for(duration_key: &str) -> i64 {
    match duration_key {
        "half_day" => 4 * 60,
        "full_day" => 9 * 60,
        "weekend" => 2 * 24 * 60,
        _ => 2 * 60,
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\r', "").replace('\n', "\\n")
}

/// Lines over 75 octets continue on the next line after a space.
fn fold(line: &str) -> String {
    let mut out = String::new();
    let mut len = 0;
    for ch in line.chars() {
        let w = ch.len_utf8();
        if len + w > 75 {
            out.push_str("\r\n ");
            len = 1;
        }
        out.push(ch);
        len += w;
    }
    out
}

/// "20261018T133000Z"
fn utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_valid_event() {
        let ics = render(&Event {
            uid: "joy-7@joy".into(),
            start: 1_792_330_200, // 2026-10-18 13:30 UTC = 19:00 IST
            minutes: 120,
            summary: "Dinner at Saltt, then the drive; maybe",
            location: "Karjat",
            description: "line one\nline two",
            now: 1_792_300_000,
        });
        assert!(ics.contains("DTSTART:20261018T133000Z\r\n"));
        assert!(ics.contains("DTEND:20261018T153000Z\r\n"));
        assert!(ics.contains("SUMMARY:Dinner at Saltt\\, then the drive\\; maybe\r\n"));
        assert!(ics.contains("DESCRIPTION:line one\\nline two\r\n"));
        assert!(ics.ends_with("END:VCALENDAR\r\n"));
        assert!(!ics.replace("\r\n", "").contains('\n'), "only CRLF line breaks");
    }

    #[test]
    fn folds_long_lines() {
        let long = "x".repeat(200);
        let folded = fold(&format!("SUMMARY:{long}"));
        assert!(folded.split("\r\n").all(|l| l.len() <= 75));
        assert_eq!(folded.replace("\r\n ", ""), format!("SUMMARY:{long}"));
    }

    #[test]
    fn utc_dates() {
        assert_eq!(utc(0), "19700101T000000Z");
        assert_eq!(utc(951_782_400), "20000229T000000Z");
    }
}
