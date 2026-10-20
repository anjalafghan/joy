use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{Form, SignedCookieJar};
use serde::Deserialize;

use super::{budget_view, planned_input, planned_label, render, AppError, BudgetView, HtmlResult, Me};
use crate::{
    auth,
    db::{self, HistoryRow},
    ics, photos,
    picker::Duration,
    routes::decide::sealed_for,
    weather, AppState,
};

/// One kept date as the viewer is allowed to see it.
pub struct KeptView {
    pub id: i64,
    /// A surprise for this viewer: everything below except `hints`, `from_name`,
    /// `planned_label` and `day_label` is blank.
    pub sealed: bool,
    pub title: String,
    /// Setting key ("home", "local", …) for colour. The hints already say this much.
    pub setting: String,
    pub place: String,
    /// "YYYY-MM-DD"
    pub day: String,
    pub day_label: String,
    pub rating: Option<i64>,
    pub note: String,
    /// Who planned it ("Rohit"), when we know.
    pub from_name: String,
    /// On the planner's own device, for an unrevealed surprise: who it's for ("Neha").
    pub surprise_for: String,
    /// For a sealed surprise: what to know without spoiling it.
    pub hints: Vec<String>,
    /// "Sat 18 Oct, 7:00 pm", or empty when no time is set.
    pub planned_label: String,
    /// Value for `<input type="datetime-local">`.
    pub planned_input: String,
    /// Calendar invite, when a time is set.
    pub ics_url: String,
    pub photo_url: Option<String>,
    /// "₹1,000": the slip's estimate.
    pub cost_est: String,
    /// "₹5,000", or empty when the real cost isn't filled in.
    pub cost_actual: String,
    /// Value for the cost field ("5000" or "").
    pub cost_actual_raw: String,
    /// "₹4,000 more", "₹200 less" or "spot on"; empty without a real cost.
    pub cost_diff: String,
    pub cost_over: bool,
    /// Per person: (name, real one-way cab fare as entered or ""), for the cost editor.
    pub cabs: Vec<(String, String)>,
    /// Comma-separated history ids that share a time: this one, plus its pair if it was
    /// kept as a pair, so "change time" moves both.
    pub pair_ids: String,
}

impl KeptView {
    pub fn is_rated(&self, n: &i64) -> bool {
        self.rating == Some(*n)
    }
}

/// History rows kept together as a pair with `r` (same request, so within a few seconds).
fn pair_ids(r: &HistoryRow, done: &[HistoryRow], pairs: &[(i64, i64)]) -> String {
    let paired = |x: i64, y: i64| pairs.contains(&(x.min(y), x.max(y)));
    std::iter::once(r.id)
        .chain(
            done.iter()
                .filter(|o| o.id != r.id && (o.picked_at - r.picked_at).abs() <= 5 && paired(o.activity_id, r.activity_id))
                .map(|o| o.id),
        )
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn view(s: &AppState, r: &HistoryRow, who: Option<usize>, rain: bool, pair_ids: String) -> KeptView {
    let sealed = sealed_for(r, who);
    let planned = r.planned_local.as_deref().map(planned_label).unwrap_or_default();
    let from_name = r.drawn_by.and_then(|d| s.cfg.people.get(d as usize)).map(|p| p.0.clone()).unwrap_or_default();
    let mine_unrevealed = r.surprise && !r.revealed && !sealed;
    KeptView {
        id: r.id,
        sealed,
        title: if sealed { String::new() } else { s.cfg.personalize(&r.title) },
        setting: r.setting.clone(),
        place: if sealed { String::new() } else { r.place.clone() },
        day: r.day.clone(),
        day_label: r.day_label(),
        rating: if sealed { None } else { r.rating },
        note: if sealed { String::new() } else { r.note.clone() },
        surprise_for: if mine_unrevealed {
            r.drawn_by.and_then(|d| s.cfg.people.get(1 - d as usize)).map(|p| p.0.clone()).unwrap_or_default()
        } else {
            String::new()
        },
        hints: if sealed { hints(r, &planned, rain) } else { vec![] },
        planned_input: r.planned_local.as_deref().map(planned_input).unwrap_or_default(),
        ics_url: if r.planned_at.is_some() { format!("/history/{}/invite.ics", r.id) } else { String::new() },
        // Versioned by the file's modified time, so a replaced photo never shows stale from cache.
        photo_url: (r.has_photo && !sealed).then(|| {
            let v = std::fs::metadata(photos::path(&s.cfg.photos_dir, r.id))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            format!("/photos/{}?v={v}", r.id)
        }),
        pair_ids,
        cost_est: if sealed { String::new() } else { s.cfg.money(r.est_cost) },
        cost_actual: r.actual_cost.filter(|_| !sealed).map(|c| s.cfg.money(c)).unwrap_or_default(),
        cost_actual_raw: r.actual_cost.filter(|_| !sealed).map(|c| c.to_string()).unwrap_or_default(),
        cost_diff: match r.actual_cost.filter(|_| !sealed).map(|c| c - r.est_cost) {
            None => String::new(),
            Some(0) => "spot on".into(),
            Some(d) if d > 0 => format!("{} more", s.cfg.money(d)),
            Some(d) => format!("{} less", s.cfg.money(-d)),
        },
        cost_over: !sealed && r.actual_cost.is_some_and(|c| c > r.est_cost),
        cabs: s
            .cfg
            .people
            .iter()
            .zip([r.cab_fare_0, r.cab_fare_1])
            .map(|((name, _), fare)| (name.clone(), fare.filter(|_| !sealed).map(|f| f.to_string()).unwrap_or_default()))
            .collect(),
        planned_label: planned,
        from_name,
    }
}

/// What the other one of us gets to know about a surprise.
fn hints(r: &HistoryRow, planned: &str, rain: bool) -> Vec<String> {
    let tags: Vec<&str> = r.tags.split(',').map(str::trim).collect();
    let mut h = vec![if planned.is_empty() { "When: still being planned".to_string() } else { format!("When: {planned}") }];
    h.push(format!("Keep {} free", Duration::parse(&r.duration).unwrap_or(Duration::Short).label().to_lowercase()));
    h.push(
        match r.setting.as_str() {
            "home" => "We're staying in",
            "day_trip" => "We're heading out of town",
            "travel" => "Pack a bag",
            _ => "We're going out",
        }
        .into(),
    );
    if tags.contains(&"romantic") {
        h.push("Dress up a little".into());
    } else if r.energy == "high" || tags.contains(&"active") {
        h.push("Wear comfy shoes".into());
    }
    if tags.contains(&"outdoors") {
        h.push(if rain { "Bring an umbrella" } else { "Bring a light jacket" }.into());
    }
    if tags.contains(&"foodie") {
        h.push("Come hungry".into());
    }
    h
}

pub struct Month {
    pub label: String,
    pub items: Vec<KeptView>,
}

#[derive(Template)]
#[template(path = "history.html")]
struct HistoryTpl {
    /// Kept dates, newest month first.
    months: Vec<Month>,
    kept: usize,
    skipped: Vec<HistoryRow>,
    budget: BudgetView,
    /// Who's holding this device ("Rohit"), empty if not chosen yet.
    me_name: String,
    /// Shown when a photo upload failed and JS wasn't there to say so.
    photo_error: String,
    /// "₹", for the cost field.
    currency: String,
    /// "Your dates usually run about 40% over the estimate (5 dates).", once there's enough to say.
    cost_insight: String,
}

#[derive(Deserialize)]
pub struct PageQuery {
    #[serde(default)]
    err: String,
}

pub async fn page(State(s): State<AppState>, jar: SignedCookieJar, Query(q): Query<PageQuery>) -> HtmlResult {
    let who = auth::who(&jar);
    let rain = s.weather.tonight().await.is_some_and(|p| p >= weather::LIKELY_PERCENT);
    let (done, skipped): (Vec<_>, Vec<_>) =
        db::history(&s.db).await?.into_iter().partition(|r| r.outcome == "did_it");
    // A slip that's now a sealed surprise for this viewer mustn't show up in the
    // "put back" footnote either, or it would give the surprise away.
    let sealed_titles: Vec<&str> = done.iter().filter(|r| sealed_for(r, who)).map(|r| r.title.as_str()).collect();
    let skipped: Vec<HistoryRow> = skipped
        .into_iter()
        .filter(|r| !sealed_titles.contains(&r.title.as_str()))
        .map(|mut r| {
            r.title = s.cfg.personalize(&r.title);
            r
        })
        .collect();
    let kept = done.len();
    let pairs = db::pairs(&s.db).await?;
    let mut months: Vec<Month> = vec![];
    for row in &done {
        let label = row.month_label();
        let v = view(&s, row, who, rain, pair_ids(row, &done, &pairs));
        match months.last_mut() {
            Some(m) if m.label == label => m.items.push(v),
            _ => months.push(Month { label, items: vec![v] }),
        }
    }
    render(HistoryTpl {
        months,
        kept,
        skipped,
        budget: budget_view(&s).await?,
        me_name: Me::new(&s.cfg, who).name,
        photo_error: photo_error_text(&q.err).into(),
        currency: s.cfg.currency.clone(),
        cost_insight: cost_insight(&super::learned(&s).await?),
    })
}

#[derive(Deserialize)]
pub struct RateForm {
    #[serde(default)]
    rating: String,
    #[serde(default)]
    note: String,
}

pub async fn rate(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Form(f): Form<RateForm>,
) -> Result<Response, AppError> {
    db::rate(&s.db, id, f.rating.parse().ok(), &f.note).await?;
    // htmx saves in place and needs no page back; plain forms land back on their slip.
    Ok(if auth::is_htmx(&headers) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        Redirect::to(&format!("/history#k{id}")).into_response()
    })
}

// ── when is it? ────────────────────────────────────────────────────────────

#[derive(Template)]
#[template(path = "planned.html")]
struct PlannedTpl {
    /// Same comma-separated ids that were posted, so the form can be changed again.
    ids: String,
    /// "Sat 18 Oct, 7:00 pm", or empty after clearing.
    label: String,
    /// Value for `<input type="datetime-local">`.
    input: String,
    ics_url: String,
    /// The invite to send the other one of us: sealed (no spoilers) for a surprise.
    send_url: String,
    /// The other one of us ("Neha").
    partner: String,
}

#[derive(Deserialize)]
pub struct WhenForm {
    /// Comma-separated history ids (two for a pair).
    ids: String,
    /// From `<input type="datetime-local">`: "2026-10-18T19:00", or empty to clear.
    #[serde(default)]
    at: String,
}

pub async fn when(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    headers: HeaderMap,
    Form(f): Form<WhenForm>,
) -> Result<Response, AppError> {
    let ids: Vec<i64> = f.ids.split(',').filter_map(|i| i.trim().parse().ok()).collect();
    let local = parse_local(&f.at);
    if !f.at.trim().is_empty() && local.is_none() {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, "That date and time didn't look right.").into_response());
    }
    db::set_planned(&s.db, &ids, local.as_deref()).await?;
    let first = ids.first().copied().unwrap_or_default();
    if !auth::is_htmx(&headers) {
        return Ok(Redirect::to(&format!("/history#k{first}")).into_response());
    }
    let row = db::history_row(&s.db, first).await?;
    let planned_local = row.as_ref().and_then(|r| r.planned_local.clone());
    Ok(render(PlannedTpl {
        ids: f.ids,
        label: planned_local.as_deref().map(planned_label).unwrap_or_default(),
        input: planned_local.as_deref().map(planned_input).unwrap_or_default(),
        ics_url: if planned_local.is_some() { format!("/history/{first}/invite.ics") } else { String::new() },
        send_url: match &row {
            Some(r) if planned_local.is_some() && r.surprise && !r.revealed => format!("/history/{first}/invite.ics?sealed=1"),
            Some(_) if planned_local.is_some() => format!("/history/{first}/invite.ics"),
            _ => String::new(),
        },
        partner: Me::new(&s.cfg, auth::who(&jar)).partner,
    })?
    .into_response())
}

/// "2026-10-18T19:00" → "2026-10-18 19:00" (local time), checked for shape.
fn parse_local(at: &str) -> Option<String> {
    let at = at.trim();
    let ok = at.len() >= 16
        && at.as_bytes()[4] == b'-'
        && at.as_bytes()[7] == b'-'
        && matches!(at.as_bytes()[10], b'T' | b' ')
        && at.as_bytes()[13] == b':'
        && at[..16].chars().enumerate().all(|(i, c)| [4, 7, 10, 13].contains(&i) || c.is_ascii_digit());
    ok.then(|| format!("{} {}", &at[..10], &at[11..16]))
}

#[derive(Deserialize)]
pub struct InviteQuery {
    /// "1": the no-spoilers version of a surprise, for sending to the other one of us.
    #[serde(default)]
    sealed: String,
}

pub async fn invite(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    Path(id): Path<i64>,
    Query(q): Query<InviteQuery>,
) -> Result<Response, AppError> {
    let who = auth::who(&jar);
    let Some(r) = db::history_row(&s.db, id).await?.filter(|r| r.planned_at.is_some()) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let sealed = sealed_for(&r, who) || (q.sealed == "1" && r.surprise && !r.revealed);
    let from = r.drawn_by.and_then(|d| s.cfg.people.get(d as usize)).map(|p| p.0.as_str()).unwrap_or("us");
    let (summary, location, description) = if sealed {
        (format!("A surprise from {from}"), String::new(), hints(&r, "", false).join("\n"))
    } else {
        let maps = r.lat.zip(r.lng).map(|(a, b)| format!("\nhttps://www.google.com/maps/search/?api=1&query={a},{b}"));
        (s.cfg.personalize(&r.title), r.place.clone(), format!("{}{}", s.cfg.personalize(&r.notes), maps.unwrap_or_default()))
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let body = ics::render(&ics::Event {
        uid: format!("joy-{id}@joy"),
        start: r.planned_at.unwrap_or(now),
        minutes: ics::minutes_for(&r.duration),
        summary: &summary,
        location: &location,
        description: &description,
        now,
    });
    Ok((
        [
            (header::CONTENT_TYPE, "text/calendar; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"joy-date.ics\""),
        ],
        body,
    )
        .into_response())
}

pub async fn delete(State(s): State<AppState>, jar: SignedCookieJar, Path(id): Path<i64>) -> Result<Response, AppError> {
    let Some(row) = db::history_row(&s.db, id).await? else {
        return Ok(Redirect::to("/history").into_response());
    };
    // A surprise still sealed for you isn't yours to throw away.
    if sealed_for(&row, auth::who(&jar)) {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    if row.has_photo {
        let _ = tokio::fs::remove_file(photos::path(&s.cfg.photos_dir, id)).await;
    }
    db::delete_history(&s.db, id).await?;
    Ok(Redirect::to("/history").into_response())
}

/// What the real costs say, in plain words.
fn cost_insight(l: &super::Learned) -> String {
    if l.cost_samples < db::MIN_SAMPLES {
        return String::new();
    }
    let pct = ((l.cost_ratio - 1.0) * 100.0).round() as i64;
    let how = match pct {
        -5..=5 => "usually cost about what the slips say".to_string(),
        p if p > 0 => format!("usually run about {p}% over the estimate"),
        p => format!("usually come in about {}% under the estimate", -p),
    };
    let mut line = format!("Your dates {how} ({} dates with real costs).", l.cost_samples);
    if l.cost.is_some() && !(-5..=5).contains(&pct) {
        line.push_str(" The jar now adjusts estimates to match.");
    }
    line
}

#[derive(Deserialize)]
pub struct CostForm {
    /// What it really cost; blank clears it.
    #[serde(default)]
    actual: String,
    /// "1": also set the slip's estimate to this.
    #[serde(default)]
    update_estimate: String,
    /// Real one-way cab fares per person (blank = didn't take one / don't know).
    #[serde(default)]
    cab_0: String,
    #[serde(default)]
    cab_1: String,
}

pub async fn set_cost(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    Path(id): Path<i64>,
    Form(f): Form<CostForm>,
) -> Result<Response, AppError> {
    let Some(row) = db::history_row(&s.db, id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if sealed_for(&row, auth::who(&jar)) {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    let amount = |v: &str| -> Result<Option<i64>, ()> {
        let raw: String = v.trim().trim_start_matches(s.cfg.currency.as_str()).chars().filter(|c| !matches!(c, ',' | ' ')).collect();
        if raw.is_empty() {
            return Ok(None);
        }
        raw.parse::<i64>().ok().filter(|v| *v >= 0).map(Some).ok_or(())
    };
    let (Ok(actual), Ok(cab_0), Ok(cab_1)) = (amount(&f.actual), amount(&f.cab_0), amount(&f.cab_1)) else {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, "That amount didn't look right.").into_response());
    };
    db::set_actual(&s.db, id, actual, f.update_estimate == "1").await?;
    // Each real fare is saved next to what the model would have said for that trip (uncalibrated),
    // so later changes to rates or home locations can't skew what we learn.
    let km = s.cfg.distances(row.lat.zip(row.lng));
    for (person, fare) in [(0, cab_0), (1, cab_1)] {
        let model = km[person].map(|k| crate::travel::cab(k).rupees);
        db::set_cab_fare(&s.db, id, person, fare.filter(|f| *f > 0), model).await?;
    }
    Ok(Redirect::to(&format!("/history#k{id}")).into_response())
}

pub async fn reveal(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Redirect, AppError> {
    db::reveal(&s.db, id).await?;
    Ok(Redirect::to(&format!("/history#k{id}")))
}

// ── photos ────────────────────────────────────────────────────────────────

fn photo_error_text(code: &str) -> &'static str {
    match code {
        "heic" => "That's an HEIC photo, which can't be read here. Choose “Most Compatible” in camera settings, or send a screenshot.",
        "big" => "That photo is over 15 MB. Try a smaller one.",
        "bad" => "That photo couldn't be used. Try another one.",
        _ => "",
    }
}

fn photo_error_code(message: &str) -> &'static str {
    if message.contains("HEIC") {
        "heic"
    } else if message.contains("15 MB") {
        "big"
    } else {
        "bad"
    }
}

/// Multipart field `photo`. With htmx/fetch: 204 on success, 422 + message on failure.
/// Without: redirects back to the slip (with ?err= on failure).
pub async fn upload_photo(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    Path(id): Path<i64>,
    headers: HeaderMap,
    mut form: Multipart,
) -> Result<Response, AppError> {
    let scripted = auth::is_htmx(&headers) || headers.get("x-requested-with").is_some();
    let fail = |msg: &'static str| -> Response {
        if scripted {
            (StatusCode::UNPROCESSABLE_ENTITY, msg).into_response()
        } else {
            Redirect::to(&format!("/history?err={}#k{id}", photo_error_code(msg))).into_response()
        }
    };
    let Some(row) = db::history_row(&s.db, id).await?.filter(|r| r.outcome == "did_it") else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if sealed_for(&row, auth::who(&jar)) {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    let mut bytes = None;
    while let Ok(Some(field)) = form.next_field().await {
        if field.name() == Some("photo") {
            match field.bytes().await {
                Ok(b) => bytes = Some(b),
                Err(_) => return Ok(fail("That photo is over 15 MB. Try a smaller one.")),
            }
        }
    }
    let Some(bytes) = bytes.filter(|b| !b.is_empty()) else {
        return Ok(fail("That file doesn't look like a photo."));
    };
    let jpeg = match tokio::task::spawn_blocking(move || photos::process(&bytes)).await? {
        Ok(j) => j,
        Err(e) => return Ok(fail(e.0)),
    };
    tokio::fs::write(photos::path(&s.cfg.photos_dir, id), jpeg).await?;
    db::set_photo(&s.db, id, true).await?;
    Ok(if scripted { StatusCode::NO_CONTENT.into_response() } else { Redirect::to(&format!("/history#k{id}")).into_response() })
}

pub async fn delete_photo(State(s): State<AppState>, jar: SignedCookieJar, Path(id): Path<i64>) -> Result<Response, AppError> {
    let Some(row) = db::history_row(&s.db, id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    if sealed_for(&row, auth::who(&jar)) {
        return Ok(StatusCode::FORBIDDEN.into_response());
    }
    let _ = tokio::fs::remove_file(photos::path(&s.cfg.photos_dir, id)).await;
    db::set_photo(&s.db, id, false).await?;
    Ok(Redirect::to(&format!("/history#k{id}")).into_response())
}

#[derive(Deserialize)]
pub struct PhotoQuery {
    v: Option<String>,
}

/// The `?v=` a photo link carries: its file's modified time.
fn view_version(s: &AppState, id: i64) -> String {
    std::fs::metadata(photos::path(&s.cfg.photos_dir, id))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
        .to_string()
}

pub async fn photo(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    Path(id): Path<i64>,
    Query(q): Query<PhotoQuery>,
) -> Result<Response, AppError> {
    let row = db::history_row(&s.db, id).await?;
    let Some(row) = row.filter(|r| r.has_photo && !sealed_for(r, auth::who(&jar))) else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let Ok(bytes) = tokio::fs::read(photos::path(&s.cfg.photos_dir, row.id)).await else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    // The URL is versioned by the file's modified time, so a matching ?v= can be cached for good.
    let current = view_version(&s, row.id);
    let cache = if q.v.as_deref() == Some(current.as_str()) { "private, max-age=31536000, immutable" } else { "private, no-cache" };
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, cache)], bytes).into_response())
}

// ── monthly budget ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BudgetForm {
    /// Blank clears it.
    #[serde(default)]
    amount: String,
    /// Where to go afterwards ("/" or "/history").
    #[serde(default)]
    back: String,
}

pub async fn set_budget(State(s): State<AppState>, Form(f): Form<BudgetForm>) -> Result<Redirect, AppError> {
    let amount: Option<i64> = f.amount.trim().replace(',', "").parse().ok().filter(|a| *a > 0);
    let value = amount.map(|a| a.to_string());
    db::set_setting(&s.db, "monthly_budget", value.as_deref()).await?;
    let back = if f.back.starts_with('/') && !f.back.starts_with("//") { f.back } else { "/history".into() };
    Ok(Redirect::to(&back))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_datetime_local() {
        assert_eq!(parse_local("2026-10-18T19:00").as_deref(), Some("2026-10-18 19:00"));
        assert_eq!(parse_local("2026-10-18T19:00:30").as_deref(), Some("2026-10-18 19:00"));
        assert_eq!(parse_local("tomorrow"), None);
        assert_eq!(parse_local("2026-10-1819:00"), None);
    }
}
