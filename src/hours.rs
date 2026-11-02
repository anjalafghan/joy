//! Opening hours as we write them on a slip: "tue-sun 12:00-23:30; mon 18:00-01:00".
//! Days are three-letter keys, as a range ("tue-sun"), a list ("sat,sun") or one day; a close
//! earlier than the open runs past midnight. Minutes count from local midnight.

use std::fmt;

const DAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
const DAY: u32 = 24 * 60;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Span {
    /// Bit d set = open on weekday d (0 = Sunday).
    days: u8,
    open: u32,
    /// May be past 1440 when it runs past midnight.
    close: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hours {
    spans: Vec<Span>,
}

fn day(s: &str) -> Option<u32> {
    DAYS.iter().position(|d| *d == s.trim()).map(|d| d as u32)
}

/// "mon-fri" → Mon..Fri; "fri-mon" wraps through the weekend; "sat,sun" lists.
fn days(s: &str) -> Option<u8> {
    let mut mask = 0u8;
    for part in s.split(',') {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (day(a)?, day(b)?);
                let mut d = a;
                loop {
                    mask |= 1 << d;
                    if d == b {
                        break;
                    }
                    d = (d + 1) % 7;
                }
            }
            None => mask |= 1 << day(part)?,
        }
    }
    (mask != 0).then_some(mask)
}

/// "9:05" / "09:05" → 545. "24:00" is allowed as a close.
fn clock(s: &str) -> Option<u32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h <= 24 && m < 60 && h * 60 + m <= DAY).then_some(h * 60 + m)
}

impl Hours {
    /// None for empty or anything we can't read, so a typo never closes a place by accident.
    pub fn parse(s: &str) -> Option<Hours> {
        let s = s.trim().to_lowercase();
        if s.is_empty() || s.len() > 200 {
            return None;
        }
        let mut spans = vec![];
        for group in s.split(';').map(str::trim).filter(|g| !g.is_empty()) {
            let (d, t) = group.split_once(char::is_whitespace)?;
            let (open, close) = t.trim().split_once('-')?;
            let (open, mut close) = (clock(open)?, clock(close)?);
            if open >= DAY {
                return None;
            }
            // "23:59" means midnight, so a place open all day really is.
            if close == DAY - 1 {
                close = DAY;
            }
            if close <= open {
                close += DAY;
            }
            spans.push(Span { days: days(d)?, open, close });
        }
        (!spans.is_empty()).then_some(Hours { spans })
    }

    /// Every stretch it's open on `weekday`, in that day's minutes (a span carried over from
    /// the night before starts below zero).
    fn stretches(&self, weekday: u8) -> impl Iterator<Item = (i64, i64)> + '_ {
        let yesterday = (weekday + 6) % 7;
        self.spans.iter().flat_map(move |s| {
            let today = (s.days >> weekday & 1 == 1).then_some((s.open as i64, s.close as i64));
            let carried = (s.days >> yesterday & 1 == 1 && s.close > DAY)
                .then_some((s.open as i64 - DAY as i64, s.close as i64 - DAY as i64));
            today.into_iter().chain(carried)
        })
    }

    /// Open for the whole of [from, to) on `weekday`? `to` may run past midnight.
    pub fn covers(&self, weekday: u8, from: i64, to: i64) -> bool {
        self.stretches(weekday).any(|(a, b)| a <= from && to <= b)
    }

    /// Open at any time on `weekday`.
    pub fn open_on(&self, weekday: u8) -> bool {
        self.spans.iter().any(|s| s.days >> weekday & 1 == 1)
    }

    /// The earliest start at or after `from` when it's open for `minutes` straight, ending by `latest_end`.
    pub fn first_fit(&self, weekday: u8, from: i64, minutes: i64, latest_end: i64) -> Option<i64> {
        self.stretches(weekday)
            .map(|(a, b)| (a.max(from), b.min(latest_end)))
            .filter(|(start, end)| end - start >= minutes)
            .map(|(start, _)| start)
            .min()
    }

    /// When it closes, if it's open at `at` on `weekday`.
    pub fn closes(&self, weekday: u8, at: i64) -> Option<i64> {
        self.stretches(weekday).filter(|(a, b)| *a <= at && at < *b).map(|(_, b)| b).max()
    }

    /// "Open 12:00 pm – 11:30 pm" for `weekday`, or "Closed on Mondays".
    pub fn label(&self, weekday: u8) -> String {
        let today: Vec<String> = self
            .spans
            .iter()
            .filter(|s| s.days >> weekday & 1 == 1)
            .map(|s| if s.open == 0 && s.close == DAY { "all day".into() } else { format!("{} – {}", clock_label(s.open as i64), clock_label(s.close as i64)) })
            .collect();
        if today.is_empty() {
            let name = crate::picker::WEEKDAYS.get(weekday as usize).map_or("that day", |d| d.1);
            format!("Closed on {name}s")
        } else {
            format!("Open {}", today.join(", "))
        }
    }
}

/// 1170 → "7:30 pm"; 1440 → "midnight"; past midnight wraps.
pub fn clock_label(minutes: i64) -> String {
    let m = minutes.rem_euclid(DAY as i64);
    if m == 0 && minutes > 0 {
        return "midnight".into();
    }
    let (h, min) = (m / 60, m % 60);
    let (h12, ampm) = match h {
        0 => (12, "am"),
        1..=11 => (h, "am"),
        12 => (12, "pm"),
        _ => (h - 12, "pm"),
    };
    format!("{h12}:{min:02} {ampm}")
}

impl fmt::Display for Hours {
    /// The canonical form we store: day ranges where they run on, 24-hour times.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let groups: Vec<String> = self
            .spans
            .iter()
            .map(|s| {
                let close = if s.close == DAY { DAY - 1 } else { s.close % DAY };
                format!("{} {:02}:{:02}-{:02}:{:02}", days_label(s.days), s.open / 60, s.open % 60, close / 60, close % 60)
            })
            .collect();
        f.write_str(&groups.join("; "))
    }
}

/// Bitmask → "mon-fri", "sat,sun" or "mon-sun".
fn days_label(mask: u8) -> String {
    if mask == 0x7f {
        return "mon-sun".into();
    }
    // Walk Monday-first so "mon-fri" and "fri-sun" read naturally.
    let order = [1u8, 2, 3, 4, 5, 6, 0];
    let mut runs: Vec<(u8, u8)> = vec![];
    for &d in &order {
        if mask >> d & 1 == 0 {
            continue;
        }
        match runs.last_mut() {
            Some((_, end)) if order.iter().position(|x| x == end).map(|i| order[(i + 1) % 7]) == Some(d) => *end = d,
            _ => runs.push((d, d)),
        }
    }
    runs.iter()
        .map(|&(a, b)| if a == b { DAYS[a as usize].to_string() } else { format!("{}-{}", DAYS[a as usize], DAYS[b as usize]) })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    const TUE: u8 = 2;
    const MON: u8 = 1;

    #[test]
    fn reads_ranges_lists_and_late_nights() {
        let h = Hours::parse("tue-sun 12:00-23:30; mon 18:00-01:00").unwrap();
        assert!(h.covers(TUE, 19 * 60 + 30, 21 * 60));
        assert!(!h.covers(TUE, 23 * 60, 24 * 60), "closes 11:30");
        assert!(h.covers(MON, 22 * 60, 24 * 60 + 30), "Monday runs past midnight");
        assert!(h.covers(TUE, 0, 30), "…which Tuesday sees as the small hours");
        assert!(!h.covers(MON, 12 * 60, 13 * 60));
        assert!(h.open_on(MON) && h.open_on(0));
    }

    #[test]
    fn wraps_day_ranges_through_the_weekend() {
        let h = Hours::parse("fri-mon 10:00-18:00").unwrap();
        assert!(h.open_on(5) && h.open_on(6) && h.open_on(0) && h.open_on(1));
        assert!(!h.open_on(TUE) && !h.open_on(3) && !h.open_on(4));
        assert_eq!(h.to_string(), "mon,fri-sun 10:00-18:00");
    }

    #[test]
    fn all_day_means_all_day() {
        let h = Hours::parse("mon-sun 00:00-23:59").unwrap();
        assert!(h.covers(3, 23 * 60, 24 * 60));
        assert_eq!(h.label(3), "Open all day");
        assert_eq!(h.to_string(), "mon-sun 00:00-23:59");
    }

    #[test]
    fn nonsense_is_unknown_not_closed() {
        for s in ["", "daily", "mon 9-5", "funday 10:00-12:00", "mon 25:00-26:00", "mon 10:00"] {
            assert_eq!(Hours::parse(s), None, "{s}");
        }
    }

    #[test]
    fn finds_the_first_window_that_fits() {
        let h = Hours::parse("sat 16:00-22:00").unwrap();
        assert_eq!(h.first_fit(6, 11 * 60, 120, 21 * 60), Some(16 * 60));
        assert_eq!(h.first_fit(6, 11 * 60, 360, 21 * 60), None, "only 5 hours before we leave");
        assert_eq!(h.closes(6, 17 * 60), Some(22 * 60));
        assert_eq!(h.closes(6, 23 * 60), None);
    }

    #[test]
    fn labels_for_people() {
        let h = Hours::parse("tue-sun 12:00-23:30").unwrap();
        assert_eq!(h.label(TUE), "Open 12:00 pm – 11:30 pm");
        assert_eq!(h.label(MON), "Closed on Mondays");
        assert_eq!(clock_label(19 * 60 + 30), "7:30 pm");
        assert_eq!(clock_label(24 * 60), "midnight");
        assert_eq!(clock_label(25 * 60), "1:00 am");
    }
}
