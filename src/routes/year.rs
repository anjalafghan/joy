//! "Our 2026": a year in review, worked out on request from what's already stored.
//! Nothing new is saved; the share picture is drawn on the phone (static/year.js).
//! Design: docs/year-in-review-and-offline.md, section 3.

use std::time::UNIX_EPOCH;

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::SignedCookieJar;
use serde::Serialize;
use sqlx::FromRow;

use super::{render, AppError, Me};
use crate::{auth, db, photos, picker, AppState};

/// A kept date that happened this year and that both of us may see (no unrevealed surprises).
#[derive(Debug, Clone, FromRow)]
pub struct YearRow {
    pub id: i64,
    pub title: String,
    pub setting: String,
    pub tags: String,
    pub place: String,
    pub rating: Option<i64>,
    pub note: String,
    pub has_photo: bool,
    pub est: i64,
    pub actual: Option<i64>,
    pub cab_0: Option<i64>,
    pub cab_1: Option<i64>,
    /// 1–12, local time.
    pub month: i64,
    /// Local julian day number, for week streaks.
    pub jd: i64,
    /// "16 Oct"
    pub day_label: String,
}

async fn rows(db: &sqlx::SqlitePool, year: i32) -> sqlx::Result<Vec<YearRow>> {
    sqlx::query_as(
        "SELECT h.id, a.title, a.setting, a.tags,
                CASE WHEN a.venue != '' THEN a.venue ELSE a.area END AS place,
                h.rating, h.note, h.has_photo,
                COALESCE(h.est_cost, a.est_cost) AS est, h.actual_cost AS actual, h.cab_fare_0 AS cab_0, h.cab_fare_1 AS cab_1,
                CAST(strftime('%m', COALESCE(h.planned_at, h.picked_at), 'unixepoch', 'localtime') AS INTEGER) AS month,
                CAST(julianday(date(COALESCE(h.planned_at, h.picked_at), 'unixepoch', 'localtime')) AS INTEGER) AS jd,
                CAST(strftime('%d', COALESCE(h.planned_at, h.picked_at), 'unixepoch', 'localtime') AS INTEGER) || ' ' ||
                  substr('JanFebMarAprMayJunJulAugSepOctNovDec',
                         1 + 3 * (CAST(strftime('%m', COALESCE(h.planned_at, h.picked_at), 'unixepoch', 'localtime') AS INTEGER) - 1), 3) AS day_label
         FROM history h JOIN activities a ON a.id = h.activity_id
         WHERE h.outcome = 'did_it'
           AND (h.surprise = 0 OR h.revealed = 1)
           AND COALESCE(h.planned_at, h.picked_at) <= unixepoch()
           AND CAST(strftime('%Y', COALESCE(h.planned_at, h.picked_at), 'unixepoch', 'localtime') AS INTEGER) = ?
         ORDER BY COALESCE(h.planned_at, h.picked_at), h.id",
    )
    .bind(year)
    .fetch_all(db)
    .await
}

/// Dates this year still to come, and (for the planner only) surprises still sealed.
async fn later(db: &sqlx::SqlitePool, year: i32, who: Option<usize>) -> sqlx::Result<(i64, i64)> {
    let to_come: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM history WHERE outcome = 'did_it' AND (surprise = 0 OR revealed = 1)
           AND planned_at > unixepoch()
           AND CAST(strftime('%Y', planned_at, 'unixepoch', 'localtime') AS INTEGER) = ?",
    )
    .bind(year)
    .fetch_one(db)
    .await?;
    let sealed_mine: i64 = match who {
        Some(w) => {
            sqlx::query_scalar(
                "SELECT count(*) FROM history WHERE outcome = 'did_it' AND surprise = 1 AND revealed = 0 AND drawn_by = ?
                   AND CAST(strftime('%Y', COALESCE(planned_at, picked_at), 'unixepoch', 'localtime') AS INTEGER) = ?",
            )
            .bind(w as i64)
            .bind(year)
            .fetch_one(db)
            .await?
        }
        None => 0,
    };
    Ok((to_come, sealed_mine))
}

pub struct MonthBar {
    /// "J", "F", … for the strip; `name` for screen readers.
    pub letter: String,
    pub name: String,
    pub n: i64,
    /// 0–100, relative to the busiest month.
    pub pct: i64,
}

pub struct BestNight {
    pub title: String,
    pub when: String,
    pub rating: i64,
    /// First line of the note (page only, never in the share picture).
    pub note: String,
    /// Other nights with the same top rating.
    pub more: usize,
    /// Its photo, when it has one.
    pub photo: String,
}

/// One kind of night (home, local, …) and its share of the year, for the paper strip.
pub struct Kind {
    pub key: String,
    pub label: String,
    pub n: usize,
    /// 1–100 of the year's dates, at least 1 so every kind shows.
    pub pct: i64,
}

/// A month's blot on the money line.
pub struct MonthBlot {
    pub letter: String,
    pub name: String,
    /// Radius in px, 0 when nothing was spent.
    pub r: String,
    pub over: bool,
    /// "March: ₹6,800 of ₹6,000", for screen readers.
    pub text: String,
}

pub struct Money {
    /// What the dates with a real cost came to, e.g. "₹42,300".
    pub spent: String,
    /// "₹3,100 under what we guessed", "₹1,800 over", "right on what we guessed".
    pub diff: String,
    pub over: bool,
    /// How many dates had a real cost.
    pub n: usize,
}

/// A folded slip lying in the header's jar (one per date, coloured by kind of night).
pub struct JarSlip {
    pub x: i64,
    pub y: i64,
    pub rot: i64,
    pub kind: String,
}

/// "★★★★★ × 3" under "stars we gave".
pub struct RatingRow {
    pub stars: i64,
    pub n: usize,
}

pub struct YearPhoto {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub when: String,
}

/// Exactly what the share picture may contain: no names, home areas, notes or app address.
/// Places, money and photos are only used if the person ticks them on the page.
#[derive(Serialize)]
pub struct ShareData {
    pub year: i32,
    pub dates: usize,
    pub months: Vec<i64>,
    pub vibe: String,
    pub tags: Vec<String>,
    pub best_title: String,
    pub best_rating: i64,
    /// Other nights with the same top rating ("and 4 more").
    pub best_more: usize,
    /// "home", "local", "day_trip" or "travel", for the picture's tint.
    pub vibe_key: String,
    pub avg_rating: String,
    pub streak_weeks: i64,
    /// Only drawn when "show where we went" is ticked.
    pub places: Vec<String>,
    /// Only drawn when "show what we spent" is ticked.
    pub spent: String,
    /// Photo ids → versioned URLs; only drawn when ticked one by one.
    pub photos: Vec<(i64, String)>,
}

#[derive(Template)]
#[template(path = "year.html")]
struct YearTpl {
    me_name: String,
    year: i32,
    /// Years with kept dates, newest first (for "← 2025").
    years: Vec<i32>,
    dates: usize,
    to_come: i64,
    /// Planner only: their own surprises this year, still sealed.
    sealed_mine: i64,
    months: Vec<MonthBar>,
    busiest: String,
    vibe_label: String,
    vibe_key: String,
    tags: Vec<String>,
    best: Option<BestNight>,
    avg_rating: String,
    rated: usize,
    streak_weeks: i64,
    money: Option<Money>,
    /// "₹2,400 in cabs", when any real fares were logged.
    cabs: String,
    photos: Vec<YearPhoto>,
    /// How many dates the busiest month held.
    busiest_n: i64,
    /// Tally marks: groups of five (the last group may be shorter), at most 24 groups.
    tally: Vec<u8>,
    /// Dates beyond what the tally has room for.
    tally_more: usize,
    kinds: Vec<Kind>,
    /// "our best kind of night: day trips (4.8★)", or empty.
    best_kind: String,
    /// "4 five-star nights, 3 four-star", or empty.
    rating_line: String,
    /// The two most-given ratings, most stars first.
    rating_rows: Vec<RatingRow>,
    /// At most 14 slips in the header's jar.
    jar_slips: Vec<JarSlip>,
    month_blots: Vec<MonthBlot>,
    /// Everything spent this year, slips or not; empty when nothing was.
    year_total: String,
    /// "₹6,000 a month", when a monthly budget is set.
    budget_note: String,
    /// Pre-escaped JSON for <script type="application/json" id="year-share">.
    share_json: String,
}

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// `/year` → this year (or last year, in January when this one has barely started).
pub async fn current(State(s): State<AppState>) -> Result<Redirect, AppError> {
    let (y, m): (i32, i32) = sqlx::query_as(
        "SELECT CAST(strftime('%Y', 'now', 'localtime') AS INTEGER), CAST(strftime('%m', 'now', 'localtime') AS INTEGER)",
    )
    .fetch_one(&s.db)
    .await?;
    Ok(Redirect::to(&format!("/year/{}", if m == 1 { y - 1 } else { y })))
}

pub async fn page(State(s): State<AppState>, jar: SignedCookieJar, Path(year): Path<i32>) -> Result<Response, AppError> {
    if !(2000..=2100).contains(&year) {
        return Ok(Redirect::to("/year").into_response());
    }
    let who = auth::who(&jar);
    let rows = rows(&s.db, year).await?;
    let (to_come, sealed_mine) = later(&s.db, year, who).await?;
    // Same rule as the recap itself: dates that happened and that both of us may see, so a year
    // whose only date is a sealed surprise doesn't show up in the switcher.
    let years: Vec<i32> = sqlx::query_scalar(
        "SELECT DISTINCT CAST(strftime('%Y', COALESCE(planned_at, picked_at), 'unixepoch', 'localtime') AS INTEGER) AS y
         FROM history WHERE outcome = 'did_it' AND (surprise = 0 OR revealed = 1)
           AND COALESCE(planned_at, picked_at) <= unixepoch()
         ORDER BY y DESC",
    )
    .fetch_all(&s.db)
    .await?;
    let r = Recap::from(&rows);
    let money = r.money.map(|(spent, guessed, n)| {
        let d = spent - guessed;
        Money {
            spent: s.cfg.money(spent),
            diff: match d {
                0 => "right on what we guessed".into(),
                d if d > 0 => format!("{} over what we guessed", s.cfg.money(d)),
                d => format!("{} under what we guessed", s.cfg.money(-d)),
            },
            over: d > 0,
            n,
        }
    });
    let photos: Vec<YearPhoto> = rows
        .iter()
        .filter(|x| x.has_photo)
        .map(|x| YearPhoto { id: x.id, url: photo_url(&s, x.id), title: s.cfg.personalize(&x.title), when: x.day_label.clone() })
        .collect();
    let best = r.best.map(|(i, more)| {
        let b = &rows[i];
        BestNight {
            title: s.cfg.personalize(&b.title),
            when: b.day_label.clone(),
            rating: b.rating.unwrap_or(0),
            note: b.note.lines().next().unwrap_or("").to_string(),
            more,
            photo: if b.has_photo { photo_url(&s, b.id) } else { String::new() },
        }
    });
    let share = ShareData {
        year,
        dates: rows.len(),
        months: r.months.to_vec(),
        vibe: r.vibe.as_deref().map(|v| picker::setting_label(v).to_string()).unwrap_or_default(),
        tags: r.tags.clone(),
        best_title: best.as_ref().map(|_| share_safe(&s, &rows[r.best.unwrap().0].title)).unwrap_or_default(),
        best_rating: best.as_ref().map_or(0, |b| b.rating),
        best_more: best.as_ref().map_or(0, |b| b.more),
        vibe_key: r.vibe.clone().unwrap_or_default(),
        avg_rating: r.avg_rating.clone(),
        streak_weeks: r.streak_weeks,
        places: {
            let mut p: Vec<String> = rows.iter().map(|x| share_safe(&s, &x.place)).filter(|p| !p.is_empty()).collect();
            p.dedup();
            p.truncate(8);
            p
        },
        spent: money.as_ref().map(|m| m.spent.clone()).unwrap_or_default(),
        photos: photos.iter().map(|p| (p.id, p.url.clone())).collect(),
    };
    let max = r.months.iter().copied().max().unwrap_or(0).max(1);
    let busiest = r.months.iter().enumerate().max_by_key(|(i, n)| (**n, -(*i as i64))).filter(|(_, n)| **n > 0).map(|(i, _)| MONTHS[i].to_string());
    let busiest_n = r.months.iter().copied().max().unwrap_or(0);
    let tally: Vec<u8> = (0..rows.len().min(120)).step_by(5).map(|i| (rows.len().min(120) - i).min(5) as u8).collect();
    let kinds: Vec<Kind> = r
        .kinds
        .iter()
        .map(|(k, n)| Kind {
            key: k.clone(),
            label: picker::setting_label(k).to_string(),
            n: *n,
            pct: ((*n * 100 / rows.len().max(1)) as i64).max(1),
        })
        .collect();
    let best_kind = r
        .best_kind
        .as_ref()
        .map(|(k, avg)| format!("our best kind of night: {} ({avg}★)", picker::setting_label(k).to_lowercase()))
        .unwrap_or_default();
    let rating_line = r
        .rating_hist
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, n)| **n > 0)
        .take(2)
        .map(|(i, n)| format!("{n} {}-star", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let rating_rows: Vec<RatingRow> = r
        .rating_hist
        .iter()
        .enumerate()
        .rev()
        .filter(|(_, n)| **n > 0)
        .take(2)
        .map(|(i, n)| RatingRow { stars: i as i64 + 1, n: *n })
        .collect();
    // Slips settle in rows from the bottom of the jar; positions are fixed so the jar never jumps.
    const SLIP_SPOTS: [(i64, i64, i64); 14] = [
        (52, 246, -14), (96, 252, 6), (140, 240, -8), (62, 262, 3), (150, 258, 12), (78, 224, 20), (124, 218, -26),
        (50, 206, 9), (156, 210, -4), (100, 196, -12), (136, 186, 17), (66, 182, -20), (112, 166, 5), (150, 160, -9),
    ];
    let jar_slips: Vec<JarSlip> = rows
        .iter()
        .zip(SLIP_SPOTS)
        .map(|(x, (sx, sy, rot))| JarSlip { x: sx, y: sy, rot, kind: x.setting.clone() })
        .collect();
    let spent = db::spent_by_month(&s.db, year).await?;
    let standing: Option<i64> =
        db::setting(&s.db, "monthly_budget").await?.and_then(|v| v.parse().ok()).filter(|b: &i64| *b > 0);
    let mut month_blots = vec![];
    let top = spent.iter().copied().max().unwrap_or(0).max(1);
    for (i, sp) in spent.iter().copied().enumerate() {
        let budget = db::month_budget(&s.db, &format!("{year}-{:02}", i + 1)).await?.map(|(b, c)| b + c).or(standing);
        let r_px = match (sp, budget) {
            (0, _) => 0.0,
            (sp, Some(b)) => (sp as f64 / b as f64 * 9.0).clamp(3.0, 13.0),
            (sp, None) => (sp as f64 / top as f64 * 12.0).clamp(3.0, 13.0),
        };
        month_blots.push(MonthBlot {
            letter: MONTHS[i][..1].into(),
            name: MONTHS[i].into(),
            r: format!("{r_px:.1}"),
            over: budget.is_some_and(|b| sp > b),
            text: match (sp, budget) {
                (0, _) => format!("{}: nothing logged", MONTHS[i]),
                (sp, Some(b)) => format!("{}: {} of {}", MONTHS[i], s.cfg.money(sp), s.cfg.money(b)),
                (sp, None) => format!("{}: {}", MONTHS[i], s.cfg.money(sp)),
            },
        });
    }
    let year_spent: i64 = spent.iter().sum();
    Ok(render(YearTpl {
        busiest_n,
        tally_more: rows.len().saturating_sub(120),
        tally,
        kinds,
        best_kind,
        rating_line,
        rating_rows,
        jar_slips,
        month_blots,
        year_total: if year_spent > 0 { s.cfg.money(year_spent) } else { String::new() },
        budget_note: standing.map(|b| format!("{} a month", s.cfg.money(b))).unwrap_or_default(),
        me_name: Me::new(&s.cfg, who).name,
        year,
        years,
        dates: rows.len(),
        to_come,
        sealed_mine,
        months: r
            .months
            .iter()
            .enumerate()
            .map(|(i, n)| MonthBar { letter: MONTHS[i][..1].into(), name: MONTHS[i].into(), n: *n, pct: n * 100 / max })
            .collect(),
        busiest: busiest.unwrap_or_default(),
        vibe_label: r.vibe.as_deref().map(|v| picker::setting_label(v).to_string()).unwrap_or_default(),
        vibe_key: r.vibe.unwrap_or_default(),
        tags: r.tags,
        best,
        avg_rating: r.avg_rating,
        rated: r.rated,
        streak_weeks: r.streak_weeks,
        money,
        cabs: if r.cabs > 0 { format!("{} in cabs", s.cfg.money(r.cabs)) } else { String::new() },
        photos,
        share_json: serde_json::to_string(&share)?.replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026"),
    })?
    .into_response())
}

fn photo_url(s: &AppState, id: i64) -> String {
    let v = std::fs::metadata(photos::path(&s.cfg.photos_dir, id))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    format!("/photos/{id}?v={v}")
}

/// Text that's safe to put in a picture others may see: personal placeholders and our names
/// become "us", home areas are dropped (old slips may carry them literally).
fn share_safe(s: &AppState, text: &str) -> String {
    let mut t = text.replace("{a}", "us").replace("{b}", "us");
    // Optional [[…]] blocks only ever hold home-area hints: drop them.
    while let (Some(a), Some(b)) = (t.find("[["), t.find("]]")) {
        if b < a {
            break;
        }
        t.replace_range(a..b + 2, "");
    }
    t = t.replace("{a_area}", "").replace("{b_area}", "");
    for (name, _) in &s.cfg.people {
        t = replace_word(&t, name, "us");
    }
    for area in s.cfg.areas.iter().flatten() {
        t = t.replace(area.as_str(), "");
    }
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Replace whole-word, case-insensitive occurrences of `word`.
fn replace_word(text: &str, word: &str, with: &str) -> String {
    if word.is_empty() {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    let w = word.to_lowercase();
    let mut out = String::new();
    let mut i = 0;
    while let Some(pos) = lower[i..].find(&w).map(|p| p + i) {
        let before_ok = lower[..pos].chars().last().is_none_or(|c| !c.is_alphanumeric());
        let after_ok = lower[pos + w.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric());
        out.push_str(&text[i..pos]);
        out.push_str(if before_ok && after_ok { with } else { &text[pos..pos + w.len()] });
        i = pos + w.len();
    }
    out.push_str(&text[i..]);
    out
}

/// The numbers, worked out from this year's rows (pure; unit-tested).
#[derive(Debug, Default)]
pub struct Recap {
    pub months: [i64; 12],
    pub vibe: Option<String>,
    pub tags: Vec<String>,
    /// (index into rows, how many other nights share the top rating)
    pub best: Option<(usize, usize)>,
    pub avg_rating: String,
    pub rated: usize,
    pub streak_weeks: i64,
    /// (spent, what we guessed for those, how many) over dates with a real cost.
    pub money: Option<(i64, i64, usize)>,
    pub cabs: i64,
    /// Every kind of night with its count, most common first.
    pub kinds: Vec<(String, usize)>,
    /// How many nights got 1, 2, … 5 stars.
    pub rating_hist: [usize; 5],
    /// The kind with the best average rating (key, "4.8"), only when at least two kinds have two
    /// or more rated nights to compare.
    pub best_kind: Option<(String, String)>,
}

impl Recap {
    pub fn from(rows: &[YearRow]) -> Self {
        let mut r = Recap::default();
        for x in rows {
            if let Some(m) = usize::try_from(x.month - 1).ok().filter(|m| *m < 12) {
                r.months[m] += 1;
            }
            r.cabs += x.cab_0.unwrap_or(0) + x.cab_1.unwrap_or(0);
        }
        let count = |items: Vec<String>| -> Vec<(String, usize)> {
            let mut c: Vec<(String, usize)> = vec![];
            for i in items {
                match c.iter_mut().find(|(k, _)| *k == i) {
                    Some(e) => e.1 += 1,
                    None => c.push((i, 1)),
                }
            }
            c.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            c
        };
        r.kinds = count(rows.iter().map(|x| x.setting.clone()).collect());
        r.vibe = r.kinds.first().map(|(k, _)| k.clone());
        for x in rows {
            if let Some(v) = x.rating.filter(|v| (1..=5).contains(v)) {
                r.rating_hist[(v - 1) as usize] += 1;
            }
        }
        let mut by_kind: Vec<(String, f64, usize)> = vec![];
        for (kind, _) in &r.kinds {
            let rated: Vec<i64> = rows.iter().filter(|x| x.setting == *kind).filter_map(|x| x.rating).collect();
            if rated.len() >= 2 {
                by_kind.push((kind.clone(), rated.iter().sum::<i64>() as f64 / rated.len() as f64, rated.len()));
            }
        }
        if by_kind.len() >= 2 {
            by_kind.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.2.cmp(&a.2)));
            r.best_kind = by_kind.first().map(|(k, avg, _)| (k.clone(), format!("{avg:.1}")));
        }
        r.tags = count(
            rows.iter()
                .flat_map(|x| x.tags.split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect::<Vec<_>>())
                .collect(),
        )
        .into_iter()
        .take(3)
        .map(|(k, _)| k)
        .collect();
        let rated: Vec<(usize, i64)> = rows.iter().enumerate().filter_map(|(i, x)| x.rating.map(|v| (i, v))).collect();
        r.rated = rated.len();
        if let Some(top) = rated.iter().map(|(_, v)| *v).max() {
            let tops: Vec<usize> = rated.iter().filter(|(_, v)| *v == top).map(|(i, _)| *i).collect();
            // The most recent of the best nights.
            r.best = tops.last().map(|i| (*i, tops.len() - 1));
            let avg = rated.iter().map(|(_, v)| *v as f64).sum::<f64>() / rated.len() as f64;
            r.avg_rating = format!("{avg:.1}");
        }
        // Longest run of consecutive weeks (Monday-based) with at least one date.
        let mut weeks: Vec<i64> = rows.iter().map(|x| (x.jd + 1).div_euclid(7)).collect();
        weeks.sort_unstable();
        weeks.dedup();
        let (mut best, mut run) = (0, 0);
        for (i, w) in weeks.iter().enumerate() {
            run = if i > 0 && *w == weeks[i - 1] + 1 { run + 1 } else { 1 };
            best = best.max(run);
        }
        r.streak_weeks = best;
        let costed: Vec<&YearRow> = rows.iter().filter(|x| x.actual.is_some()).collect();
        if !costed.is_empty() {
            r.money = Some((
                costed.iter().map(|x| x.actual.unwrap_or(0)).sum(),
                costed.iter().map(|x| x.est).sum(),
                costed.len(),
            ));
        }
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(month: i64, jd: i64, setting: &str, tags: &str, rating: Option<i64>, actual: Option<i64>) -> YearRow {
        YearRow {
            id: jd,
            title: format!("t{jd}"),
            setting: setting.into(),
            tags: tags.into(),
            place: String::new(),
            rating,
            note: String::new(),
            has_photo: false,
            est: 1000,
            actual,
            cab_0: Some(200),
            cab_1: None,
            month,
            jd,
            day_label: String::new(),
        }
    }

    #[test]
    fn recap_numbers() {
        // jd 2461000 is a Monday-based week boundary-agnostic run: three weeks in a row, a gap, then one.
        let rows = vec![
            row(3, 2461000, "local", "foodie,cozy", Some(4), Some(1500)),
            row(3, 2461007, "local", "foodie", Some(5), None),
            row(3, 2461014, "home", "cozy,games", Some(5), Some(800)),
            row(7, 2461100, "local", "foodie", None, None),
        ];
        let r = Recap::from(&rows);
        assert_eq!(r.months[2], 3);
        assert_eq!(r.months[6], 1);
        assert_eq!(r.vibe.as_deref(), Some("local"));
        assert_eq!(r.tags, vec!["foodie", "cozy", "games"]);
        assert_eq!(r.best, Some((2, 1)), "most recent five-star night, and one more like it");
        assert_eq!(r.avg_rating, "4.7");
        assert_eq!(r.streak_weeks, 3);
        assert_eq!(r.money, Some((2300, 2000, 2)), "only dates with a real cost are compared");
        assert_eq!(r.cabs, 800);
        assert_eq!(r.rating_hist, [0, 0, 0, 1, 2]);
        assert_eq!(r.kinds, vec![("local".to_string(), 3), ("home".to_string(), 1)]);
        assert_eq!(r.best_kind, None, "home has only one rated night, so there's nothing to compare");
    }

    #[test]
    fn best_kind_needs_two_kinds_with_two_rated_nights_each() {
        let rows = vec![
            row(1, 2461000, "local", "", Some(3), None),
            row(1, 2461001, "local", "", Some(4), None),
            row(2, 2461040, "travel", "", Some(5), None),
            row(2, 2461041, "travel", "", Some(5), None),
        ];
        assert_eq!(Recap::from(&rows).best_kind, Some(("travel".into(), "5.0".into())));
    }

    #[test]
    fn whole_word_name_replacement() {
        assert_eq!(replace_word("watch Neha react", "Neha", "us"), "watch us react");
        assert_eq!(replace_word("Nehal's place", "Neha", "us"), "Nehal's place");
        assert_eq!(replace_word("neha & NEHA", "Neha", "us"), "us & us");
    }
}
