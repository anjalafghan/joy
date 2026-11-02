//! When we can actually meet, and whether a slip fits one of those dates.
//!
//! Set in .env (see README): which weekdays we meet, the office we leave from on weeknights,
//! our weekend hours, and the latest each of us can be somewhere on the way home. From those,
//! every slip either fits a date (with the times worked out) or says why it doesn't.

use time::{Date, Month, Weekday};

use crate::{
    config::{road_km, Config},
    db::Activity,
    hours::{clock_label, Hours},
    travel,
};

const DAY: i64 = 24 * 60;

/// Where one of us has to be by when, heading home on a weeknight.
#[derive(Debug, Clone)]
pub struct Latest {
    /// Minutes after midnight (may pass 1440).
    pub by: i64,
    /// A place on the way ("Goregaon station"), or None for home.
    pub at: Option<(f64, f64)>,
    pub place: String,
}

#[derive(Debug, Clone)]
pub struct Schedule {
    /// Bit d = we meet on weekday d (0 = Sunday).
    pub days: u8,
    pub office: Option<(f64, f64)>,
    pub office_name: String,
    pub weeknight_start: i64,
    pub weekend_start: i64,
    pub weekend_end: i64,
    pub latest: [Latest; 2],
    /// Each of our usual home → office trips in minutes, to calibrate the travel model.
    pub commute: [Option<i64>; 2],
    /// "train", "metro": how each of us gets around.
    pub modes: [String; 2],
    /// The longest either of us will travel to a weekend date, one way.
    pub weekend_max_travel: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// After work, leaving the office together.
    Weeknight,
    /// A whole day, each of us coming from home.
    Weekend,
    /// Not a day we meet: long-distance slips only.
    Apart,
}

/// One date we could draw for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub date: Date,
    pub kind: Kind,
    pub today: bool,
}

const WEEKDAY_NAMES: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

fn weekday_index(w: Weekday) -> u8 {
    w.number_days_from_sunday()
}

/// "2026-11-05" → a date.
pub fn parse_date(s: &str) -> Option<Date> {
    let mut p = s.trim().splitn(3, '-');
    let y: i32 = p.next()?.parse().ok()?;
    let m: u8 = p.next()?.parse().ok()?;
    let d: u8 = p.next()?.parse().ok()?;
    Date::from_calendar_date(y, Month::try_from(m).ok()?, d).ok()
}

impl Slot {
    /// "2026-11-05", the value the form round-trips.
    pub fn key(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.date.year(), self.date.month() as u8, self.date.day())
    }

    pub fn weekday(&self) -> u8 {
        weekday_index(self.date.weekday())
    }

    pub fn month(&self) -> u8 {
        self.date.month() as u8
    }

    /// "this evening", "Thursday evening", "Saturday", "tonight, apart".
    pub fn label(&self) -> String {
        let name = WEEKDAY_NAMES[self.weekday() as usize];
        match (self.kind, self.today) {
            (Kind::Weeknight, true) => "this evening".into(),
            (Kind::Weeknight, false) => format!("{name} evening"),
            (Kind::Weekend, true) => "today".into(),
            (Kind::Weekend, false) => name.into(),
            (Kind::Apart, _) => "tonight, apart".into(),
        }
    }

    /// The local time SQLite expects for `planned_at`: "2026-11-05 19:30".
    pub fn local(&self, minutes: i64) -> String {
        let (mut date, mut m) = (self.date, minutes);
        while m >= DAY {
            date = date.next_day().unwrap_or(date);
            m -= DAY;
        }
        format!("{:04}-{:02}-{:02} {:02}:{:02}", date.year(), date.month() as u8, date.day(), m / 60, m % 60)
    }
}

/// "19:30" → 1170. Hours may run past 24 for a time after midnight ("25:00").
fn parse_clock(s: &str) -> Option<i64> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m) = (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?);
    (h >= 0 && (0..60).contains(&m)).then_some(h * 60 + m)
}

/// One way of getting somewhere.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leg {
    pub minutes: i64,
    /// "walk", "cab", "train", "metro".
    pub by: &'static str,
}

/// How a slip fits a date: when it starts and ends, and how each of us gets there and back.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    /// Minutes after midnight on the slot's day.
    pub start: i64,
    pub end: i64,
    /// Less time than the slip would like; still worth it.
    pub tight: bool,
    /// To the place: from the office (both of us together) or from each home.
    pub there: [Option<Leg>; 2],
    /// Back: to wherever each of us has to be.
    pub back: [Option<Leg>; 2],
}

/// A short hop between two places in town: walk, cab or train, whichever's quickest.
pub fn hop(km: f64) -> Leg {
    if km < 1.2 {
        return Leg { minutes: round5((km / 4.5 * 60.0) as i64 + 3), by: "walk" };
    }
    let (cab, train) = (travel::cab(km), travel::train(km));
    if train.minutes < cab.minutes {
        Leg { minutes: train.minutes, by: "train" }
    } else {
        Leg { minutes: cab.minutes, by: "cab" }
    }
}

fn round5(m: i64) -> i64 {
    ((m + 4) / 5 * 5).max(5)
}

/// "1:00 pm"-style label for a time, or "" for none.
pub fn at(minutes: i64) -> String {
    clock_label(minutes)
}

const MODES: [&str; 3] = ["train", "metro", "cab"];

impl Schedule {
    /// What .env says, or None when DATE_DAYS isn't set (the jar then works as before).
    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> Option<Schedule> {
        let days = var("DATE_DAYS")?;
        let mask = days
            .split(',')
            .filter_map(|d| crate::picker::WEEKDAYS.iter().position(|(k, _)| *k == d.trim().to_lowercase()))
            .fold(0u8, |m, d| m | 1 << d);
        if mask == 0 {
            return None;
        }
        let clock = |k: &str, default: i64| var(k).and_then(|v| parse_clock(&v)).filter(|m| *m <= DAY + 6 * 60).unwrap_or(default);
        let (weekend_start, weekend_end) = var("WEEKEND_HOURS")
            .and_then(|v| {
                let (a, b) = v.split_once('-')?;
                Some((parse_clock(a)?, parse_clock(b)?)).filter(|(a, b)| a < b && *b <= DAY)
            })
            .unwrap_or((11 * 60, 21 * 60));
        let latest = |who: &str| Latest {
            by: clock(&format!("LATEST_{who}"), 23 * 60),
            at: var(&format!("LATEST_{who}_AT")).and_then(|s| crate::config::parse_latlng(&s)),
            place: var(&format!("LATEST_{who}_PLACE")).unwrap_or_default(),
        };
        let mode = |who: &str| var(&format!("TRAVEL_{who}")).map(|m| m.to_lowercase()).filter(|m| MODES.contains(&m.as_str())).unwrap_or_else(|| "train".into());
        let minutes = |k: &str| var(k).and_then(|v| v.trim().parse::<i64>().ok()).filter(|m| (5..=360).contains(m));
        Some(Schedule {
            days: mask,
            office: var("OFFICE").and_then(|s| crate::config::parse_latlng(&s)),
            office_name: var("OFFICE_NAME").unwrap_or_else(|| "the office".into()),
            weeknight_start: clock("WEEKNIGHT_START", 19 * 60 + 30),
            weekend_start,
            weekend_end,
            latest: [latest("A"), latest("B")],
            commute: [minutes("COMMUTE_A_MIN"), minutes("COMMUTE_B_MIN")],
            modes: [mode("A"), mode("B")],
            weekend_max_travel: minutes("WEEKEND_MAX_TRAVEL_MIN").unwrap_or(150),
        })
    }

    pub fn meets_on(&self, weekday: u8) -> bool {
        self.days >> weekday & 1 == 1
    }

    /// The next few dates we could draw for, starting today. Today counts until it's too late
    /// to start something; on a day we don't meet, today is a long-distance evening.
    pub fn slots(&self, today: Date, now_min: i64, n: usize) -> Vec<Slot> {
        let mut out = vec![];
        let wd = weekday_index(today.weekday());
        if !self.meets_on(wd) && now_min < 23 * 60 {
            out.push(Slot { date: today, kind: Kind::Apart, today: true });
        }
        let mut d = today;
        for i in 0..14 {
            let w = weekday_index(d.weekday());
            if self.meets_on(w) {
                let kind = if w == 0 || w == 6 { Kind::Weekend } else { Kind::Weeknight };
                let too_late = match kind {
                    Kind::Weeknight => now_min > self.weeknight_start + 45,
                    _ => now_min > self.weekend_end - 3 * 60,
                };
                if i > 0 || !too_late {
                    out.push(Slot { date: d, kind, today: i == 0 });
                }
            }
            if out.len() >= n {
                break;
            }
            d = match d.next_day() {
                Some(n) => n,
                None => break,
            };
        }
        out.truncate(n);
        out
    }

    /// Calibration for one of us: our real commute ÷ what the model thinks it is.
    fn factor(&self, cfg: &Config, who: usize) -> f64 {
        match (self.commute[who], cfg.people[who].1, self.office) {
            (Some(real), Some(home), Some(office)) => {
                let model = travel::train(road_km(home, office)).minutes.max(10) as f64;
                (real as f64 / model).clamp(0.6, 3.0)
            }
            _ => 1.0,
        }
    }

    /// From one of our homes to a place (or back), the way that person travels.
    fn home_leg(&self, cfg: &Config, who: usize, place: (f64, f64)) -> Option<Leg> {
        let home = cfg.people[who].1?;
        let km = road_km(home, place);
        let base = if self.modes[who] == "cab" { travel::cab(km).minutes } else { travel::train(km).minutes };
        let by = match self.modes[who].as_str() {
            "metro" => "metro",
            "cab" => "cab",
            _ => "train",
        };
        Some(Leg { minutes: round5((base as f64 * self.factor(cfg, who)) as i64), by })
    }

    /// Does this slip fit the slot, and how? Err says why not, in a few words.
    pub fn fit(&self, cfg: &Config, slot: &Slot, a: &Activity) -> Result<Fit, String> {
        let minutes = a.minutes();
        let hours = Hours::parse(&a.hours);
        let wd = slot.weekday();
        if let Some(h) = &hours {
            if !h.open_on(wd) {
                return Err(format!("closed on {}s", WEEKDAY_NAMES[wd as usize]));
            }
        }
        match slot.kind {
            Kind::Apart => {
                if a.setting == "apart" {
                    Ok(Fit { start: 21 * 60, end: 21 * 60 + minutes.min(180), tight: false, there: [None; 2], back: [None; 2] })
                } else {
                    Err("you're apart that day".into())
                }
            }
            _ if a.setting == "apart" => Err("for the days you're apart".into()),
            _ if a.setting == "travel" || minutes >= DAY => Err("needs a whole weekend away".into()),
            Kind::Weeknight => self.weeknight(cfg, a, minutes, hours.as_ref(), wd),
            Kind::Weekend => self.weekend(cfg, a, minutes, hours.as_ref(), wd),
        }
    }

    fn weeknight(&self, cfg: &Config, a: &Activity, minutes: i64, hours: Option<&Hours>, wd: u8) -> Result<Fit, String> {
        if a.setting == "home" {
            return Err("you head home separately after work".into());
        }
        if matches!(a.best_time.as_str(), "morning" | "afternoon") {
            return Err(format!("a {} thing", a.best_time));
        }
        let Some(office) = self.office else {
            return Err("set OFFICE to plan weeknights".into());
        };
        // A slip without a place is somewhere near the office.
        let venue = a.latlng().unwrap_or(office);
        let there = hop(road_km(office, venue));
        let start = round5(self.weeknight_start + there.minutes);
        let mut back = [None; 2];
        let mut end_by = i64::MAX;
        let mut who_limits = 0;
        for (who, slot) in back.iter_mut().enumerate() {
            let l = &self.latest[who];
            let leg = match l.at {
                Some(p) => Some(hop(road_km(venue, p))),
                None => self.home_leg(cfg, who, venue),
            };
            if let Some(leg) = leg {
                if l.by - leg.minutes < end_by {
                    end_by = l.by - leg.minutes;
                    who_limits = who;
                }
                *slot = Some(leg);
            }
        }
        let end_by = if end_by == i64::MAX { 22 * 60 } else { end_by - end_by.rem_euclid(5) };
        let mut start = start;
        let mut room = end_by - start;
        if let Some(h) = hours {
            let want = minutes.min(room.max(0));
            match h.first_fit(wd, start, want.max(45), end_by) {
                Some(s) => {
                    start = round5(s);
                    room = end_by - start;
                }
                None => {
                    return Err(match h.closes(wd, start) {
                        Some(c) => format!("closes at {}", clock_label(c)),
                        None => "not open that evening".into(),
                    })
                }
            }
            if let Some(close) = h.closes(wd, start) {
                room = room.min(close - start);
            }
        }
        if room < 45 || room * 4 < minutes * 3 {
            let name = &cfg.people[who_limits].0;
            return Err(if room < 0 { format!("too far for a weeknight ({name} would be late)") } else { format!("too long for a weeknight ({name} has to head back by {})", clock_label(end_by)) });
        }
        Ok(Fit { start, end: start + minutes.min(room), tight: room < minutes, there: [Some(there); 2], back })
    }

    fn weekend(&self, cfg: &Config, a: &Activity, minutes: i64, hours: Option<&Hours>, wd: u8) -> Result<Fit, String> {
        let span = self.weekend_end - self.weekend_start;
        let minutes = minutes.min(span);
        let preferred = match a.best_time.as_str() {
            "afternoon" => 14 * 60,
            "evening" => 17 * 60 + 30,
            "night" => 19 * 60,
            _ => self.weekend_start,
        }
        .clamp(self.weekend_start, self.weekend_end - minutes);
        if a.setting == "home" {
            return Ok(Fit { start: preferred, end: preferred + minutes, tight: false, there: [None; 2], back: [None; 2] });
        }
        let mut there = [None; 2];
        if let Some(v) = a.latlng() {
            for (who, leg) in there.iter_mut().enumerate() {
                *leg = self.home_leg(cfg, who, v);
                if let Some(leg) = leg.filter(|l| l.minutes > self.weekend_max_travel) {
                    return Err(format!("{} each way for {}", travel::duration_label(leg.minutes), cfg.people[who].0));
                }
            }
        }
        let start = match hours {
            None => preferred,
            Some(h) => h
                .first_fit(wd, preferred, minutes, self.weekend_end)
                .or_else(|| h.first_fit(wd, self.weekend_start, minutes, self.weekend_end))
                .ok_or_else(|| "not open long enough that day".to_string())?,
        };
        let start = round5(start);
        Ok(Fit { start, end: (start + minutes).min(self.weekend_end), tight: false, there, back: there })
    }
}

/// One line of the day-of card.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// "7:30 pm"
    pub at: String,
    /// The same, in minutes after midnight on the day.
    pub minutes: i64,
    pub what: String,
    /// Whose step it is, when it's only one of us (for "you" vs a name).
    pub who: Option<usize>,
}

/// "train" → "by train"; walking reads as "on foot".
fn by(leg: &Leg) -> String {
    if leg.by == "walk" { "on foot".into() } else { format!("by {}", leg.by) }
}

/// The day-of timeline: when to leave, where to be, when to head back, when each of us is home.
pub fn timeline(sched: &Schedule, cfg: &Config, slot: &Slot, a: &Activity, fit: &Fit) -> Vec<Step> {
    let place = if a.venue.is_empty() { cfg.personalize(&a.title) } else { a.venue.clone() };
    let station = if !a.station.is_empty() { match a.station_walk_min {
        Some(w) => format!(" ({} station, {w} min walk)", a.station),
        None => format!(" ({} station)", a.station),
    } } else { Default::default() };
    let mut steps = vec![];
    let step = |at: i64, what: String, who: Option<usize>| Step { at: clock_label(at), minutes: at, what, who };
    match slot.kind {
        Kind::Apart => steps.push(step(fit.start, "Call, and open the slip together".into(), None)),
        Kind::Weeknight => {
            let there = fit.there[0].map_or(0, |t| t.minutes);
            steps.push(step(fit.start - there, format!("Leave {} together", sched.office_name), None));
            if let Some(t) = fit.there[0] {
                // The station only matters if that's how we get there.
                let station = if matches!(t.by, "train" | "metro") { station.as_str() } else { "" };
                steps.push(step(fit.start, format!("{place}, {} {}{station}", travel::duration_label(t.minutes), by(&t)), None));
            }
            steps.push(step(fit.end, if fit.tight { "Head back (a short one tonight)".into() } else { "Head back".into() }, None));
            let mut homes: Vec<(i64, Step)> = (0..2)
                .filter_map(|who| {
                    let leg = fit.back[who]?;
                    let l = &sched.latest[who];
                    let at = fit.end + leg.minutes;
                    let where_ = if l.place.is_empty() { "home".into() } else { l.place.clone() };
                    Some((at, step(at, format!("{} at {where_}", cfg.people[who].0), Some(who))))
                })
                .collect();
            homes.sort_by_key(|h| h.0);
            steps.extend(homes.into_iter().map(|h| h.1));
        }
        Kind::Weekend => {
            let mut leaves: Vec<(i64, Step)> = (0..2)
                .filter_map(|who| {
                    let leg = fit.there[who]?;
                    let at = fit.start - leg.minutes;
                    Some((at, step(at, format!("{} leaves home, {} {}", cfg.people[who].0, travel::duration_label(leg.minutes), by(&leg)), Some(who))))
                })
                .collect();
            leaves.sort_by_key(|l| l.0);
            steps.extend(leaves.into_iter().map(|l| l.1));
            let meet = if a.setting == "home" { format!("Together at one of your places: {}", cfg.personalize(&a.title)) } else { format!("Meet at {place}{station}") };
            steps.push(step(fit.start, meet, None));
            steps.push(step(fit.end, "Wrap up and head home".into(), None));
            let mut homes: Vec<(i64, Step)> = (0..2)
                .filter_map(|who| {
                    let leg = fit.back[who]?;
                    let at = fit.end + leg.minutes;
                    Some((at, step(at, format!("{} home", cfg.people[who].0), Some(who))))
                })
                .collect();
            homes.sort_by_key(|h| h.0);
            steps.extend(homes.into_iter().map(|h| h.1));
        }
    }
    steps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sched() -> Schedule {
        let vars = [
            ("DATE_DAYS", "tue,thu,sat,sun"),
            ("OFFICE", "19.1741, 72.8580"),
            ("OFFICE_NAME", "Cello Triumph"),
            ("WEEKNIGHT_START", "19:30"),
            ("WEEKEND_HOURS", "11:00-21:00"),
            ("LATEST_A", "22:00"),
            ("LATEST_A_AT", "19.1646, 72.8493"),
            ("LATEST_A_PLACE", "Goregaon station"),
            ("LATEST_B", "23:00"),
            ("COMMUTE_A_MIN", "120"),
            ("COMMUTE_B_MIN", "75"),
            ("TRAVEL_B", "metro"),
        ];
        Schedule::from_vars(|k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())).unwrap()
    }

    fn cfg() -> Config {
        let mut c = Config::for_tests("photos".into(), "", true);
        // Ulwe and Mira Road.
        c.people = [("Anjal".into(), Some((18.97, 73.03))), ("Diya".into(), Some((19.28, 72.87)))];
        c
    }

    fn slip(setting: &str, at: Option<(f64, f64)>, minutes: i64) -> Activity {
        Activity {
            id: 1,
            title: "Test".into(),
            notes: String::new(),
            est_cost: 0,
            setting: setting.into(),
            area: String::new(),
            venue: "Somewhere".into(),
            lat: at.map(|a| a.0),
            lng: at.map(|a| a.1),
            duration: "short".into(),
            tags: String::new(),
            energy: "med".into(),
            added_by: String::new(),
            times_done: 0,
            last_done_at: None,
            last_skipped_at: None,
            generated: false,
            closed_days: String::new(),
            season_from: None,
            season_to: None,
            est_from_actual: false,
            hours: String::new(),
            best_time: String::new(),
            minutes: Some(minutes),
            booking: String::new(),
            book_ahead_days: None,
            cover: String::new(),
            bring: String::new(),
            station: String::new(),
            station_walk_min: None,
        }
    }

    fn date(s: &str) -> Date {
        parse_date(s).unwrap()
    }

    // 2026-11-03 is a Tuesday.
    const TUE: &str = "2026-11-03";
    const SAT: &str = "2026-11-07";
    const VERSOVA: (f64, f64) = (19.135, 72.815);
    const COLABA: (f64, f64) = (18.92, 72.83);

    #[test]
    fn reads_the_schedule_from_env() {
        let s = sched();
        assert!(s.meets_on(2) && s.meets_on(4) && s.meets_on(6) && s.meets_on(0));
        assert!(!s.meets_on(1) && !s.meets_on(3) && !s.meets_on(5));
        assert_eq!((s.weeknight_start, s.weekend_start, s.weekend_end), (1170, 660, 1260));
        assert_eq!(s.latest[0].by, 22 * 60);
        assert_eq!(s.latest[0].place, "Goregaon station");
        assert!(s.latest[1].at.is_none(), "Diya's limit is home");
        assert_eq!(s.modes, ["train".to_string(), "metro".to_string()]);
        assert!(Schedule::from_vars(|_| None).is_none(), "off without DATE_DAYS");
    }

    #[test]
    fn offers_the_next_dates_and_an_apart_evening() {
        let s = sched();
        // Monday afternoon: tonight apart, then Tue, Thu, Sat.
        let slots = s.slots(date("2026-11-02"), 15 * 60, 4);
        assert_eq!(slots[0].kind, Kind::Apart);
        assert_eq!(slots[0].label(), "tonight, apart");
        assert_eq!(slots[1].key(), TUE);
        assert_eq!(slots[1].label(), "Tuesday evening");
        assert_eq!(slots[2].key(), "2026-11-05");
        assert_eq!(slots[3].kind, Kind::Weekend);
        // Tuesday at 10 pm is too late for tonight.
        let late = s.slots(date(TUE), 22 * 60, 2);
        assert_eq!(late[0].key(), "2026-11-05");
        // Tuesday at 6 pm: this evening.
        assert_eq!(s.slots(date(TUE), 18 * 60, 1)[0].label(), "this evening");
    }

    #[test]
    fn a_weeknight_near_the_office_fits_and_gets_us_back_in_time() {
        let (s, c) = (sched(), cfg());
        let slot = Slot { date: date(TUE), kind: Kind::Weeknight, today: false };
        let fit = s.fit(&c, &slot, &slip("local", Some(VERSOVA), 90)).unwrap();
        assert!(fit.start > 19 * 60 + 30 && fit.start < 20 * 60 + 30, "{fit:?}");
        let a_back = fit.end + fit.back[0].unwrap().minutes;
        let b_back = fit.end + fit.back[1].unwrap().minutes;
        assert!(a_back <= 22 * 60, "Anjal at Goregaon by 10: {a_back}");
        assert!(b_back <= 23 * 60, "Diya home by 11: {b_back}");
    }

    #[test]
    fn colaba_is_too_far_on_a_weeknight() {
        let (s, c) = (sched(), cfg());
        let slot = Slot { date: date(TUE), kind: Kind::Weeknight, today: false };
        let err = s.fit(&c, &slot, &slip("local", Some(COLABA), 120)).unwrap_err();
        assert!(err.contains("weeknight"), "{err}");
    }

    #[test]
    fn opening_hours_and_time_of_day_count() {
        let (s, c) = (sched(), cfg());
        let tue = Slot { date: date(TUE), kind: Kind::Weeknight, today: false };
        let mut early = slip("local", Some(VERSOVA), 90);
        early.hours = "mon-sun 08:00-20:00".into();
        assert!(s.fit(&c, &tue, &early).unwrap_err().contains("closes at 8:00 pm"));
        let mut closed = slip("local", Some(VERSOVA), 90);
        closed.hours = "wed-sun 12:00-23:00".into();
        assert_eq!(s.fit(&c, &tue, &closed).unwrap_err(), "closed on Tuesdays");
        let mut brunch = slip("local", Some(VERSOVA), 90);
        brunch.best_time = "morning".into();
        assert!(s.fit(&c, &tue, &brunch).is_err());
        let sat = Slot { date: date(SAT), kind: Kind::Weekend, today: false };
        assert_eq!(s.fit(&c, &sat, &brunch).unwrap().start, 11 * 60, "brunch fits a Saturday");
    }

    #[test]
    fn weekends_start_from_both_homes() {
        let (s, c) = (sched(), cfg());
        let sat = Slot { date: date(SAT), kind: Kind::Weekend, today: false };
        let mut gig = slip("local", Some((19.06, 72.83)), 150);
        gig.best_time = "evening".into();
        gig.hours = "mon-sun 18:00-01:00".into();
        let fit = s.fit(&c, &sat, &gig).unwrap();
        assert_eq!(fit.start, 18 * 60, "waits for it to open");
        assert!(fit.there[0].unwrap().minutes >= 60 && fit.there[1].unwrap().minutes >= 60, "both of us come from home: {fit:?}");
        assert_eq!(fit.there[1].unwrap().by, "metro");
        let steps = timeline(&s, &c, &sat, &gig, &fit);
        assert!(steps[0].what.contains("leaves home") && steps[1].what.contains("leaves home"), "{steps:?}");
        assert!(steps.iter().any(|st| st.what.starts_with("Meet at Somewhere")));
    }

    #[test]
    fn calibrates_travel_to_our_real_commutes() {
        let (s, c) = (sched(), cfg());
        let office = s.office.unwrap();
        assert_eq!(s.home_leg(&c, 0, office).unwrap().minutes, 120, "Anjal's usual two hours");
        assert_eq!(s.home_leg(&c, 1, office).unwrap().minutes, 75);
    }

    #[test]
    fn days_apart_are_for_long_distance_slips() {
        let (s, c) = (sched(), cfg());
        let mon = Slot { date: date("2026-11-02"), kind: Kind::Apart, today: true };
        assert!(s.fit(&c, &mon, &slip("apart", None, 60)).is_ok());
        assert!(s.fit(&c, &mon, &slip("local", Some(VERSOVA), 60)).is_err());
        let tue = Slot { date: date(TUE), kind: Kind::Weeknight, today: false };
        assert!(s.fit(&c, &tue, &slip("apart", None, 60)).is_err());
        assert!(s.fit(&c, &tue, &slip("home", None, 60)).is_err(), "no at-home dates after work");
        let mut trip = slip("travel", Some(COLABA), 2880);
        trip.duration = "weekend".into();
        let sat = Slot { date: date(SAT), kind: Kind::Weekend, today: false };
        assert_eq!(s.fit(&c, &sat, &trip).unwrap_err(), "needs a whole weekend away");
    }

    #[test]
    fn weeknight_timeline_reads_in_order() {
        let (s, c) = (sched(), cfg());
        let slot = Slot { date: date(TUE), kind: Kind::Weeknight, today: false };
        let a = slip("local", Some(VERSOVA), 90);
        let fit = s.fit(&c, &slot, &a).unwrap();
        let steps = timeline(&s, &c, &slot, &a, &fit);
        assert_eq!(steps[0].what, "Leave Cello Triumph together");
        assert_eq!(steps[0].at, "7:30 pm");
        assert!(steps.iter().any(|st| st.what == "Anjal at Goregaon station"));
        assert!(steps.iter().any(|st| st.what == "Diya at home"));
        assert_eq!(slot.local(fit.start).len(), 16);
    }
}
