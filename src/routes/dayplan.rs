//! The day-of plan for a kept date: the timeline, a Plan B, somewhere to eat after, and the
//! little jobs the jar dealt each of us. Shown under the stamp and on the jar page until we go.

use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{Form, SignedCookieJar};
use serde::Deserialize;
use time::Date;

use super::{decide::sealed_for, render, AppError};
use crate::{
    auth,
    config::{road_km, Config},
    db::{self, Activity, HistoryRow},
    hours::Hours,
    schedule::{self, Fit, Kind, Schedule, Slot},
    weather, AppState,
};

/// Every date we could draw for right now (empty without a schedule).
pub async fn slots(s: &AppState, n: usize) -> Result<Vec<Slot>, AppError> {
    let Some(sched) = &s.cfg.schedule else { return Ok(vec![]) };
    let (date, minutes) = db::local_now(&s.db).await?;
    Ok(schedule::parse_date(&date).map(|d| sched.slots(d, minutes, n)).unwrap_or_default())
}

/// Every day of the coming week, for plans that don't follow the usual days.
pub async fn days(s: &AppState) -> Result<Vec<Slot>, AppError> {
    let Some(sched) = &s.cfg.schedule else { return Ok(vec![]) };
    let (date, minutes) = db::local_now(&s.db).await?;
    Ok(schedule::parse_date(&date).map(|d| sched.days(d, minutes, 8)).unwrap_or_default())
}

/// The date a draw is for: the one the form names, else the next one, starting from where
/// the form says ("office", "home", or "" for the usual). "any" = no particular date.
pub async fn slot_for(s: &AppState, when: &str, start: &str) -> Result<Option<Slot>, AppError> {
    if when == "any" {
        return Ok(None);
    }
    let slot = match days(s).await?.into_iter().find(|x| x.key() == when) {
        Some(sl) => Some(sl),
        None => slots(s, 1).await?.into_iter().next(),
    };
    Ok(slot.map(|sl| sl.starting_from(start)))
}

/// Exactly the date the form names, if it's still one we could draw for (no falling back).
pub async fn slot_exact(s: &AppState, when: &str, start: &str) -> Result<Option<Slot>, AppError> {
    Ok(days(s).await?.into_iter().find(|x| x.key() == when).map(|sl| sl.starting_from(start)))
}

/// The kind of date a day is: what we kept it as, else what our schedule says.
pub fn slot_on(sched: &Schedule, date: Date, kept_as: &str) -> Slot {
    Slot { date, kind: Kind::parse(kept_as).unwrap_or_else(|| sched.usual(date)), today: false }
}

/// A slip to swap to on the day: indoors, close by, about the same money, and fitting the same date.
pub fn choose_plan_b(cfg: &Config, slot: Option<&Slot>, a: &Activity, acts: &[Activity]) -> Option<i64> {
    let here = a.latlng()?;
    // A free walk's Plan B can still be a café: allow a bit more than the original.
    let cap = (a.est_cost * 3 / 2).max(a.est_cost + 1000);
    let tags = a.tag_list();
    // Close by, and the same kind of evening: each shared tag counts like being 1.5 km nearer.
    let score = |b: &Activity, km: f64| km - 1.5 * b.tag_list().iter().filter(|t| tags.contains(t)).count() as f64;
    acts.iter()
        .filter(|b| b.id != a.id && b.setting == "local" && !b.outdoor() && b.est_cost <= cap)
        .filter_map(|b| Some((b, road_km(here, b.latlng()?))))
        .filter(|(_, km)| *km <= 4.0)
        .map(|(b, km)| (b, score(b, km)))
        .filter(|(b, _)| match (slot, &cfg.schedule) {
            (Some(slot), Some(sched)) => sched.fit(cfg, slot, b).is_ok(),
            _ => true,
        })
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(b, _)| b.id)
}

/// The jobs a slip needs before the date: booking it, and what to bring.
pub fn jobs(a: &Activity, place: &str, date: Option<Date>, today: Date) -> Vec<(String, String)> {
    let weekend = date.is_some_and(|d| matches!(d.weekday().number_days_from_sunday(), 0 | 6));
    let mut out = vec![];
    if a.booking == "always" || (a.booking == "weekends" && weekend) {
        let due = date
            .map(|d| {
                let ahead = a.book_ahead_days.unwrap_or(1).clamp(0, 60);
                let by = d.checked_sub(time::Duration::days(ahead)).unwrap_or(d).max(today).min(d);
                ymd(by)
            })
            .unwrap_or_default();
        out.push((format!("Book {place}"), due));
    }
    out.extend(a.bring_list().into_iter().take(4).map(|item| (format!("Bring {item}"), String::new())));
    out
}

/// Deal jobs out one at a time, starting with whoever has had fewer (on a tie, whoever didn't
/// draw, since drawing was their bit). A surprise's jobs all go to whoever planned it.
pub fn deal(jobs: Vec<(String, String)>, counts: [i64; 2], drawn_by: Option<usize>, surprise: bool) -> Vec<(usize, String, String)> {
    if surprise {
        if let Some(d) = drawn_by {
            return jobs.into_iter().map(|(t, due)| (d, t, due)).collect();
        }
    }
    let mut counts = counts;
    jobs.into_iter()
        .map(|(t, due)| {
            let who = match counts[0].cmp(&counts[1]) {
                std::cmp::Ordering::Less => 0,
                std::cmp::Ordering::Greater => 1,
                std::cmp::Ordering::Equal => drawn_by.map_or(0, |d| 1 - d),
            };
            counts[who] += 1;
            (who, t, due)
        })
        .collect()
}

fn ymd(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
}

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "2026-11-04" → "Wed 4 Nov"; today → "today".
pub fn day_label(ymd: &str, today: Date) -> String {
    match schedule::parse_date(ymd) {
        Some(d) if d == today => "today".into(),
        Some(d) => format!("{} {} {}", DAYS[d.weekday().number_days_from_sunday() as usize], d.day(), MONTHS[d.month() as usize - 1]),
        None => String::new(),
    }
}

pub struct StepView {
    pub at: String,
    pub minutes: i64,
    pub what: String,
    /// It's about the person holding the phone.
    pub mine: bool,
}

pub struct TaskView {
    pub id: i64,
    pub task: String,
    /// "by Wed 4 Nov", "by today", or empty.
    pub due: String,
    /// "Diya", or empty when it's mine.
    pub whose: String,
    pub mine: bool,
    pub done: bool,
}

pub struct PlanB {
    pub title: String,
    pub place: String,
}

/// Everything the day-of card shows.
pub struct DayPlan {
    pub history_id: i64,
    pub title: String,
    /// "Thursday evening · 5 Nov"
    pub when: String,
    pub steps: Vec<StepView>,
    pub tight: bool,
    /// "Open 12:00 pm – 11:30 pm" for that day, or a nudge to check.
    pub hours_note: String,
    pub plan_b: Option<PlanB>,
    /// Rain chance (%) when it's today, likely, and the date is outdoors.
    pub rain: Option<u8>,
    /// "Kala Ghoda Cafe, 300 m away".
    pub after: String,
    pub tasks: Vec<TaskView>,
    /// A surprise this viewer only gets hints of.
    pub sealed: bool,
    pub ics_url: String,
    /// When the date itself starts (minutes after midnight), for the calendar reminder.
    pub start: i64,
}

impl DayPlan {
    /// Minutes before the start that this viewer has to set off (0 when there's no travel).
    pub fn leave_before(&self, who: Option<usize>) -> i64 {
        // The earliest step before we're there that's about this viewer (or both of us).
        self.steps
            .iter()
            .filter(|st| st.minutes < self.start && (st.mine || who.is_none() || !st.what.contains(" leaves ")))
            .map(|st| self.start - st.minutes)
            .max()
            .unwrap_or(0)
    }

    pub fn my_open_tasks(&self) -> usize {
        self.tasks.iter().filter(|t| t.mine && !t.done).count()
    }
}

/// Somewhere to eat within a short walk that's open when the date ends.
fn after(cfg: &Config, a: &Activity, acts: &[Activity], wd: u8, end: i64) -> String {
    let Some(here) = a.latlng() else { return String::new() };
    acts.iter()
        .filter(|b| b.id != a.id && b.tag_list().iter().any(|t| t == "foodie") && b.setting == "local")
        .filter(|b| Hours::parse(&b.hours).is_none_or(|h| h.covers(wd, end, end + 60)))
        .filter_map(|b| Some((b, road_km(here, b.latlng()?))))
        .filter(|(_, km)| *km <= 1.2)
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(b, km)| {
            let name = if b.venue.is_empty() { cfg.personalize(&b.title) } else { b.venue.clone() };
            let m = ((km * 1000.0 / 50.0).round() as i64 * 50).max(50);
            format!("{name}, about {m} m away")
        })
        .unwrap_or_default()
}

/// The plan for a kept date that has a time, from this viewer's side. None without a schedule
/// or a time.
pub async fn build(s: &AppState, row: &HistoryRow, who: Option<usize>) -> Result<Option<DayPlan>, AppError> {
    let (Some(sched), Some(local)) = (&s.cfg.schedule, row.planned_local.as_deref()) else { return Ok(None) };
    let (Some(date), Some(minutes)) = (
        local.get(..10).and_then(schedule::parse_date),
        local.get(11..16).and_then(|hm| Some(hm.get(..2)?.parse::<i64>().ok()? * 60 + hm.get(3..)?.parse::<i64>().ok()?)),
    ) else {
        return Ok(None);
    };
    let Some(a) = db::get(&s.db, row.activity_id).await? else { return Ok(None) };
    let slot = slot_on(sched, date, &row.day_kind);
    let wd = slot.weekday();
    let fit = match sched.fit(&s.cfg, &slot, &a) {
        Ok(f) => {
            // We planned it for a different time than the jar would: move everything with it.
            let d = minutes - f.start;
            Fit { start: f.start + d, end: f.end + d, ..f }
        }
        Err(_) => Fit { start: minutes, end: minutes + a.minutes().min(12 * 60), tight: false, there: [None; 2], back: [None; 2] },
    };
    let sealed = sealed_for(row, who);
    let steps = schedule::timeline(sched, &s.cfg, &slot, &a, &fit);
    let steps: Vec<StepView> = steps
        .into_iter()
        .filter(|st| !sealed || st.who.is_none() || st.who == who)
        .map(|st| {
            let what = if sealed && st.who.is_none() && st.at != schedule::at(fit.start) { String::new() } else { st.what };
            // On a sealed surprise, how long and how you travel would hint at where: keep just the time.
            let what = if sealed { what.split(',').next().unwrap_or_default().to_string() } else { what };
            StepView { mine: st.who.is_some() && st.who == who, at: st.at, minutes: st.minutes, what }
        })
        .filter(|st| !st.what.is_empty())
        .map(|st| if sealed && st.at == schedule::at(fit.start) && !st.mine { StepView { what: "The surprise starts".into(), ..st } } else { st })
        .collect();
    let (today_s, _) = db::local_now(&s.db).await?;
    let today = schedule::parse_date(&today_s).unwrap_or(date);
    // Only a plan with somewhere to be needs the rest of the jar (for "after").
    let after = if sealed || slot.kind == Kind::Apart || a.latlng().is_none() {
        String::new()
    } else {
        after(&s.cfg, &a, &db::list(&s.db, false).await?, wd, fit.end)
    };
    let plan_b = match (sealed, row.plan_b) {
        (false, Some(id)) => db::get(&s.db, id).await?.map(|b| PlanB {
            title: s.cfg.personalize(&b.title),
            place: [b.venue.as_str(), b.area.as_str()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "),
        }),
        _ => None,
    };
    let rain = if date == today && a.outdoor() && !sealed {
        s.weather.tonight().await.filter(|p| *p >= weather::LIKELY_PERCENT)
    } else {
        None
    };
    let tasks = db::tasks_for(&s.db, row.id)
        .await?
        .into_iter()
        .filter(|t| !sealed || Some(t.who as usize) == who)
        .map(|t| TaskView {
            id: t.id,
            due: match day_label(&t.due, today) {
                l if l.is_empty() => String::new(),
                l => format!("by {l}"),
            },
            whose: if Some(t.who as usize) == who { String::new() } else { s.cfg.people.get(t.who as usize).map(|p| p.0.clone()).unwrap_or_default() },
            mine: Some(t.who as usize) == who,
            done: t.done_at.is_some(),
            task: s.cfg.personalize(&t.task),
        })
        .collect();
    Ok(Some(DayPlan {
        history_id: row.id,
        title: if sealed { "A surprise".into() } else { s.cfg.personalize(&row.title) },
        when: if date == today { slot_label_today(&slot) } else { format!("{} · {}", slot.label(), day_label(&ymd(date), today)) },
        tight: fit.tight && !sealed,
        hours_note: match Hours::parse(&a.hours) {
            _ if sealed || matches!(a.setting.as_str(), "home" | "apart") => String::new(),
            Some(h) => h.label(wd),
            None => "Hours not confirmed: check before you go".into(),
        },
        after,
        steps,
        plan_b,
        rain,
        tasks,
        sealed,
        ics_url: format!("/history/{}/invite.ics", row.id),
        start: fit.start,
    }))
}

fn slot_label_today(slot: &Slot) -> String {
    match slot.kind {
        Kind::Weeknight => "Tonight".into(),
        Kind::Weekend => "Today".into(),
        Kind::Apart => "Tonight, apart".into(),
    }
}

/// When a date is kept: pick its Plan B, deal its jobs, and set its time if we drew it for one.
pub async fn on_keep(s: &AppState, ids: &[i64], slot: Option<&Slot>, who: Option<usize>, surprise: bool) -> Result<(), AppError> {
    let Some(&first) = ids.first() else { return Ok(()) };
    let Some(row) = db::history_row(&s.db, first).await? else { return Ok(()) };
    let Some(a) = db::get(&s.db, row.activity_id).await? else { return Ok(()) };
    if let (Some(slot), Some(sched)) = (slot, &s.cfg.schedule) {
        if row.planned_at.is_none() {
            if let Ok(fit) = sched.fit(&s.cfg, slot, &a) {
                db::set_planned(&s.db, ids, Some(&slot.local(fit.start))).await?;
                // Remember an office Saturday or a Monday off, so the plan stays that way.
                db::set_day_kind(&s.db, ids, slot.kind.as_str()).await?;
            }
        }
    }
    let acts = db::list(&s.db, false).await?;
    let vetoed: Vec<i64> = db::vetoes(&s.db).await?.into_iter().map(|(a, _)| a).collect();
    let acts: Vec<Activity> = acts.into_iter().filter(|x| !vetoed.contains(&x.id)).collect();
    if row.plan_b.is_none() {
        db::set_plan_b(&s.db, first, choose_plan_b(&s.cfg, slot, &a, &acts)).await?;
    }
    let (today, _) = db::local_now(&s.db).await?;
    let today = schedule::parse_date(&today);
    let mut all_jobs = vec![];
    for id in ids {
        let Some(r) = db::history_row(&s.db, *id).await? else { continue };
        let Some(act) = db::get(&s.db, r.activity_id).await? else { continue };
        let place = if act.venue.is_empty() { s.cfg.personalize(&act.title) } else { act.venue.clone() };
        if let Some(today) = today {
            all_jobs.extend(jobs(&act, &place, slot.map(|x| x.date), today));
        }
    }
    let counts = db::task_counts(&s.db).await?;
    db::add_tasks(&s.db, first, &deal(all_jobs, counts, who, surprise)).await?;
    Ok(())
}

#[derive(Template)]
#[template(path = "dayplan.html")]
struct DayPlanTpl {
    plan: DayPlan,
}

/// The plan card on its own, for htmx swaps after ticking a job or swapping to Plan B.
async fn plan_fragment(s: &AppState, id: i64, who: Option<usize>, headers: &HeaderMap) -> Result<Response, AppError> {
    if !auth::is_htmx(headers) {
        return Ok(Redirect::to("/").into_response());
    }
    let Some(row) = db::history_row(&s.db, id).await? else { return Ok(StatusCode::NOT_FOUND.into_response()) };
    Ok(match build(s, &row, who).await? {
        Some(plan) => render(DayPlanTpl { plan })?.into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

#[derive(Deserialize)]
pub struct DoneForm {
    #[serde(default)]
    done: String,
}

/// Tick off (or untick) one of my jobs.
pub async fn task_done(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Form(f): Form<DoneForm>,
) -> Result<Response, AppError> {
    let who = auth::who(&jar);
    let Some(t) = db::task(&s.db, id).await? else { return Ok(StatusCode::NOT_FOUND.into_response()) };
    // Only whoever was dealt it ticks it off.
    if who != Some(t.who as usize) {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    db::set_task_done(&s.db, id, f.done == "1").await?;
    plan_fragment(&s, t.history_id, who, &headers).await
}

/// Go with Plan B instead (or back again).
pub async fn swap_plan_b(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Response, AppError> {
    let who = auth::who(&jar);
    let Some(row) = db::history_row(&s.db, id).await? else { return Ok(StatusCode::NOT_FOUND.into_response()) };
    if sealed_for(&row, who) {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    db::swap_plan_b(&s.db, id).await?;
    plan_fragment(&s, id, who, &headers).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> Date {
        schedule::parse_date(s).unwrap()
    }

    fn slip(id: i64, at: Option<(f64, f64)>, cost: i64) -> Activity {
        Activity {
            id,
            title: format!("slip {id}"),
            notes: String::new(),
            est_cost: cost,
            setting: "local".into(),
            area: String::new(),
            venue: format!("Venue {id}"),
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
            minutes: Some(90),
            booking: String::new(),
            book_ahead_days: None,
            cover: "indoor".into(),
            bring: String::new(),
            station: String::new(),
            station_walk_min: None,
        }
    }

    #[test]
    fn deals_fairly_and_keeps_surprises_secret() {
        let jobs = vec![("Book it".to_string(), String::new()), ("Bring cash".into(), String::new()), ("Bring water".into(), String::new())];
        let dealt = deal(jobs.clone(), [0, 0], Some(0), false);
        assert_eq!(dealt.iter().map(|d| d.0).collect::<Vec<_>>(), vec![1, 0, 1], "the drawer's partner goes first");
        let behind = deal(jobs.clone(), [5, 1], Some(1), false);
        assert!(behind.iter().all(|d| d.0 == 1), "catches up whoever has had fewer");
        assert!(deal(jobs, [0, 9], Some(0), true).iter().all(|d| d.0 == 0), "a surprise's jobs stay with its planner");
    }

    #[test]
    fn booking_jobs_come_due_ahead_of_the_date() {
        let mut a = slip(1, None, 2000);
        a.booking = "always".into();
        a.book_ahead_days = Some(2);
        a.bring = "cash, closed shoes".into();
        let j = jobs(&a, "Piano Man", Some(date("2026-11-05")), date("2026-11-02"));
        assert_eq!(j[0], ("Book Piano Man".to_string(), "2026-11-03".to_string()));
        assert_eq!(j[1].0, "Bring cash");
        assert_eq!(j.len(), 3);
        // Too late to book two days ahead: today, then.
        assert_eq!(jobs(&a, "x", Some(date("2026-11-05")), date("2026-11-05"))[0].1, "2026-11-05");
        // Weekend-only booking on a weekday: nothing to book.
        a.booking = "weekends".into();
        assert!(jobs(&a, "x", Some(date("2026-11-05")), date("2026-11-02")).iter().all(|j| !j.0.starts_with("Book")));
        assert!(jobs(&a, "x", Some(date("2026-11-07")), date("2026-11-02"))[0].0.starts_with("Book"));
    }

    #[test]
    fn plan_b_is_indoors_and_close() {
        let cfg = Config::for_tests("photos".into(), "", true);
        let mut beach = slip(1, Some((19.06, 72.83)), 500);
        beach.cover = "outdoor".into();
        let cafe = slip(2, Some((19.065, 72.832)), 600);
        let far = slip(3, Some((18.92, 72.83)), 400);
        let mut park = slip(4, Some((19.061, 72.831)), 0);
        park.cover = "outdoor".into();
        let pricey = slip(5, Some((19.0605, 72.8305)), 9000);
        let acts = vec![beach.clone(), cafe, far, park, pricey];
        assert_eq!(choose_plan_b(&cfg, None, &beach, &acts), Some(2));
        assert_eq!(choose_plan_b(&cfg, None, &slip(9, None, 0), &acts), None, "nothing to swap at home");
    }

    #[test]
    fn plan_b_prefers_the_same_kind_of_evening() {
        let cfg = Config::for_tests("photos".into(), "", true);
        let mut dinner = slip(1, Some((19.06, 72.83)), 2000);
        dinner.tags = "foodie,romantic".into();
        let rage = slip(2, Some((19.0605, 72.8301)), 1500);
        let mut bistro = slip(3, Some((19.07, 72.835)), 1800);
        bistro.tags = "foodie,romantic".into();
        assert_eq!(choose_plan_b(&cfg, None, &dinner, &[dinner.clone(), rage, bistro]), Some(3), "a bistro over a rage room");
    }

    #[test]
    fn labels_due_days() {
        let today = date("2026-11-02");
        assert_eq!(day_label("2026-11-04", today), "Wed 4 Nov");
        assert_eq!(day_label("2026-11-02", today), "today");
        assert_eq!(day_label("", today), "");
    }
}
