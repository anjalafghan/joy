//! One month of the year in review, as a journal spread: prints and ticket stubs for each date,
//! torn receipts for what it cost, the best night as a sticker. Read-only, like the year page.

use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::SignedCookieJar;

use super::{
    render,
    year::{photo_url, rows, MONTHS},
    AppError, Me,
};
use crate::{auth, db, picker, AppState};

/// One kept date on the spread.
pub struct Entry {
    pub title: String,
    pub place: String,
    /// "home", "local", … for the ticket's colour.
    pub kind: String,
    pub kind_label: String,
    /// "16 Oct"
    pub day: String,
    /// "16·10·26"
    pub stamp: String,
    pub photo: String,
    /// 0 when not rated.
    pub rating: i64,
    /// First line of the note.
    pub note: String,
    /// What we guessed, e.g. "₹1,500".
    pub est: String,
    /// What it really cost, empty when nobody filled it in.
    pub actual: String,
    /// "₹400 over our guess", "₹200 under our guess", or empty.
    pub diff: String,
    pub over: bool,
    /// "₹350", when any real cab fares were logged.
    pub cabs: String,
    /// -2..=2, so each piece sits at its own angle.
    pub tilt: i64,
}

/// Money spent on a date that wasn't a slip.
pub struct Other {
    pub amount: String,
    pub note: String,
    pub day: String,
}

pub struct Best {
    pub title: String,
    pub rating: i64,
    pub day: String,
}

/// A neighbouring month with dates in it.
pub struct Nav {
    pub href: String,
    pub label: String,
}

#[derive(Template)]
#[template(path = "month.html")]
struct MonthTpl {
    me_name: String,
    year: i32,
    /// "October"
    month_name: String,
    /// Letters for the sticker title: the month, then the year.
    month_letters: Vec<char>,
    year_letters: Vec<char>,
    entries: Vec<Entry>,
    others: Vec<Other>,
    best: Option<Best>,
    /// Everything spent this month, slips or not; empty when nothing was.
    spent: String,
    /// "4.3", empty when nothing was rated.
    avg_rating: String,
    prev: Option<Nav>,
    next: Option<Nav>,
    /// Dates this month still to come.
    to_come: i64,
}

pub async fn page(State(s): State<AppState>, jar: SignedCookieJar, Path((year, month)): Path<(String, String)>) -> Result<Response, AppError> {
    // Typed-in nonsense ("/year/2026/abc") goes back to the year rather than a bare 400.
    let Some(year) = year.parse::<i32>().ok().filter(|y| (2000..=2100).contains(y)) else {
        return Ok(Redirect::to("/year").into_response());
    };
    let Some(month) = month.parse::<u32>().ok().filter(|m| (1..=12).contains(m)) else {
        return Ok(Redirect::to(&format!("/year/{year}")).into_response());
    };
    let who = auth::who(&jar);
    let key = format!("{year}-{month:02}");
    let rows: Vec<_> = rows(&s.db, year).await?.into_iter().filter(|r| r.month == i64::from(month)).collect();

    let best = rows
        .iter()
        .filter(|r| r.rating.is_some())
        .max_by_key(|r| (r.rating, std::cmp::Reverse(r.id)))
        .map(|r| Best { title: s.cfg.personalize(&r.title), rating: r.rating.unwrap_or(0), day: r.day_label.clone() });
    let rated: Vec<i64> = rows.iter().filter_map(|r| r.rating).collect();
    let avg_rating = if rated.is_empty() {
        String::new()
    } else {
        format!("{:.1}", rated.iter().sum::<i64>() as f64 / rated.len() as f64)
    };

    let entries = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let cabs = r.cab_0.unwrap_or(0) + r.cab_1.unwrap_or(0);
            let (diff, over) = match r.actual {
                Some(a) if a > r.est => (format!("{} over our guess", s.cfg.money(a - r.est)), true),
                Some(a) if a < r.est => (format!("{} under our guess", s.cfg.money(r.est - a)), false),
                Some(_) => ("right on our guess".into(), false),
                None => (String::new(), false),
            };
            Entry {
                title: s.cfg.personalize(&r.title),
                place: s.cfg.personalize(&r.place),
                kind: r.setting.clone(),
                kind_label: picker::setting_label(&r.setting).to_string(),
                day: r.day_label.clone(),
                stamp: r.stamp.clone(),
                photo: if r.has_photo { photo_url(&s, r.id) } else { String::new() },
                rating: r.rating.unwrap_or(0),
                note: r.note.lines().next().unwrap_or("").to_string(),
                est: s.cfg.money(r.est),
                actual: r.actual.map(|a| s.cfg.money(a)).unwrap_or_default(),
                diff,
                over,
                cabs: if cabs > 0 { s.cfg.money(cabs) } else { String::new() },
                tilt: [-2, 1, -1, 2, 0][i % 5],
            }
        })
        .collect();

    let others = db::other_spending_in_month(&s.db, &key)
        .await?
        .into_iter()
        .map(|(amount, note, on)| Other { amount: s.cfg.money(amount), note, day: day_label(&on) })
        .collect();
    let spent = db::spent_by_month(&s.db, year).await?[month as usize - 1];

    // Months with dates either side of this one, across every year, so the spreads read like pages.
    let kept: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT strftime('%Y-%m', COALESCE(planned_at, picked_at), 'unixepoch', 'localtime') AS m
         FROM history WHERE outcome = 'did_it' AND (surprise = 0 OR revealed = 1)
           AND COALESCE(planned_at, picked_at) <= unixepoch()
         ORDER BY m",
    )
    .fetch_all(&s.db)
    .await?;
    let nav = |m: &String| Nav { href: format!("/year/{}/{}", &m[..4], m[5..].trim_start_matches('0')), label: month_label(m) };
    let prev = kept.iter().filter(|m| **m < key).next_back().map(nav);
    let next = kept.iter().find(|m| **m > key).map(nav);

    let to_come: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM history WHERE outcome = 'did_it' AND (surprise = 0 OR revealed = 1)
           AND planned_at > unixepoch() AND strftime('%Y-%m', planned_at, 'unixepoch', 'localtime') = ?",
    )
    .bind(&key)
    .fetch_one(&s.db)
    .await?;

    let month_name = MONTHS[month as usize - 1].to_string();
    Ok(render(MonthTpl {
        me_name: Me::new(&s.cfg, who).name,
        year,
        month_letters: month_name.chars().collect(),
        year_letters: year.to_string().chars().collect(),
        month_name,
        entries,
        others,
        best,
        spent: if spent > 0 { s.cfg.money(spent) } else { String::new() },
        avg_rating,
        prev,
        next,
        to_come,
    })?
    .into_response())
}

/// "2026-10" → "October 2026"
fn month_label(ym: &str) -> String {
    let m: usize = ym.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    format!("{} {}", MONTHS[m.clamp(1, 12) - 1], ym.get(..4).unwrap_or(""))
}

/// "2026-10-16" → "16 Oct"
fn day_label(ymd: &str) -> String {
    let m: usize = ymd.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    let d: u32 = ymd.get(8..10).and_then(|d| d.parse().ok()).unwrap_or(1);
    format!("{d} {}", &MONTHS[m.clamp(1, 12) - 1][..3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(month_label("2026-10"), "October 2026");
        assert_eq!(month_label("2025-01"), "January 2025");
        assert_eq!(day_label("2026-10-06"), "6 Oct");
    }
}
