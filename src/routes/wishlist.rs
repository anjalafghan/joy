use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{Form, SignedCookieJar};
use serde::Deserialize;

use super::{
    duration_opts, history::parse_local, planned_label, render, share_text, url_encode, ActivityView, AppError, HtmlResult, Me, Opt,
};
use crate::{
    auth,
    config::parse_latlng,
    db::{self, Activity, ActivityInput},
    picker::{self, Duration, ENERGIES, SETTINGS},
    AppState,
};

struct Group {
    key: String,
    label: String,
    items: Vec<ActivityView>,
}

#[derive(Template)]
#[template(path = "wishlist.html")]
struct WishlistTpl {
    me_name: String,
    groups: Vec<Group>,
    archived: Vec<ActivityView>,
    total: usize,
}

pub async fn page(State(s): State<AppState>, jar: SignedCookieJar) -> HtmlResult {
    let active = db::list(&s.db, false).await?;
    let pairs = db::pairs(&s.db).await?;
    let vetoes = db::vetoes(&s.db).await?;
    let learned = super::learned(&s).await?;
    let view = |a: &Activity| {
        let mut v = ActivityView::new(&s.cfg, a, &learned);
        v.vetoed_by = vetoes
            .iter()
            .filter(|(act, _)| *act == a.id)
            .filter_map(|(_, p)| s.cfg.people.get(*p as usize).map(|x| x.0.clone()))
            .collect::<Vec<_>>()
            .join(" & ");
        v.pairs = pairs
            .iter()
            .filter_map(|&(x, y)| if x == a.id { Some(y) } else if y == a.id { Some(x) } else { None })
            .filter_map(|other| active.iter().find(|o| o.id == other))
            .map(|o| s.cfg.personalize(&o.title))
            .collect();
        v
    };
    let groups = SETTINGS
        .iter()
        .map(|(key, label)| Group {
            key: (*key).into(),
            label: (*label).into(),
            items: active.iter().filter(|a| a.setting == *key).map(view).collect(),
        })
        .filter(|g| !g.items.is_empty())
        .collect();
    let archived = db::list(&s.db, true).await?.iter().map(|a| ActivityView::new(&s.cfg, a, &learned)).collect();
    render(WishlistTpl { me_name: me_name(&s, &jar), groups, archived, total: active.len() })
}

// ── plan it straight from a slip ───────────────────────────────────────────

#[derive(Template)]
#[template(path = "slip_planned.html")]
struct SlipPlannedTpl {
    title: String,
    /// "Mon 2 Nov, 7:00 pm".
    label: String,
    ics_url: String,
    /// The other one of us ("Diya").
    partner: String,
    wa_url: String,
    share_text: String,
    history_id: i64,
}

#[derive(Deserialize)]
pub struct PlanForm {
    /// From `<input type="datetime-local">`: "2026-11-02T19:00".
    #[serde(default)]
    at: String,
}

/// Keep a slip as a date at a time we choose, without drawing it from the jar.
pub async fn plan(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Form(f): Form<PlanForm>,
) -> Result<Response, AppError> {
    let Some(local) = parse_local(&f.at) else {
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, "Pick a day and a time.").into_response());
    };
    let Some(a) = db::get(&s.db, id).await? else {
        return Ok(StatusCode::NOT_FOUND.into_response());
    };
    let who = auth::who(&jar);
    let hid = db::accept(&s.db, id, who, false).await?;
    db::set_planned(&s.db, &[hid], Some(&local)).await?;
    if !auth::is_htmx(&headers) {
        return Ok(Redirect::to(&format!("/history#k{hid}")).into_response());
    }
    let row = db::history_row(&s.db, hid).await?;
    let label = row.and_then(|r| r.planned_local).as_deref().map(planned_label).unwrap_or_default();
    let view = ActivityView::new(&s.cfg, &a, &super::learned(&s).await?);
    let share = format!("Our date: {} · {label}\n{}", view.title, share_text(&view));
    Ok(render(SlipPlannedTpl {
        title: view.title,
        label,
        ics_url: format!("/history/{hid}/invite.ics"),
        partner: Me::new(&s.cfg, who).partner,
        wa_url: format!("https://wa.me/?text={}", url_encode(&share)),
        share_text: share,
        history_id: hid,
    })?
    .into_response())
}

#[derive(Template)]
#[template(path = "activity_form.html")]
struct FormTpl {
    id: Option<i64>,
    title: String,
    notes: String,
    est_cost: String,
    venue: String,
    area: String,
    location: String,
    tags: String,
    currency: String,
    /// "Rohit & Neha", signed on the slip.
    couple: String,
    error: String,
    settings: Vec<Opt>,
    durations: Vec<Opt>,
    energies: Vec<Opt>,
    people: Vec<Opt>,
    /// Every other slip in the jar, ticked if it pairs with this one.
    pair_opts: Vec<Opt>,
    me_name: String,
    /// Weekdays (value "mon"…, label "Mon"…), ticked if it's closed then.
    closed_opts: Vec<Opt>,
    /// Months for the season pickers; `selected` marks the current from/to.
    season_from_opts: Vec<Opt>,
    season_to_opts: Vec<Opt>,
    hours: String,
    /// "Open 12:00 pm – 11:30 pm" style preview of `hours` for today, or empty.
    hours_today: String,
    best_times: Vec<Opt>,
    minutes: String,
    bookings: Vec<Opt>,
    book_ahead_days: String,
    covers: Vec<Opt>,
    cover_unknown: bool,
    bring: String,
    station: String,
    station_walk_min: String,
    /// Open "More details" when any of them is filled in or wrong.
    details_open: bool,
}

async fn form(s: &AppState, jar: &SignedCookieJar, id: Option<i64>, i: &ActivityInput, error: &str) -> HtmlResult {
    let pair_opts = db::list(&s.db, false)
        .await?
        .iter()
        .filter(|o| Some(o.id) != id)
        .map(|o| Opt { value: o.id.to_string(), label: s.cfg.personalize(&o.title), selected: i.pairs.contains(&o.id) })
        .collect::<Vec<_>>();
    let mut pair_opts = pair_opts;
    // Current pairs first, so they're visible without scrolling.
    pair_opts.sort_by_key(|o| !o.selected);
    let people = s.cfg.people.iter().map(|(n, _)| (n.as_str(), n.as_str()));
    render(FormTpl {
        id,
        title: i.title.clone(),
        notes: i.notes.clone(),
        est_cost: if id.is_none() && i.est_cost == 0 && i.title.is_empty() { String::new() } else { i.est_cost.to_string() },
        venue: i.venue.clone(),
        area: i.area.clone(),
        location: i.location.clone(),
        tags: i.tags.clone(),
        currency: s.cfg.currency.clone(),
        couple: format!("{} & {}", s.cfg.people[0].0, s.cfg.people[1].0),
        error: error.into(),
        settings: Opt::list(SETTINGS, std::slice::from_ref(&i.setting)),
        durations: duration_opts(&i.duration),
        energies: Opt::list(ENERGIES, std::slice::from_ref(&i.energy)),
        people: {
            let mut p = Opt::list(people, std::slice::from_ref(&i.added_by));
            // Keep an author we don't know (e.g. imported "list") instead of silently overwriting it.
            if !i.added_by.is_empty() && !p.iter().any(|o| o.selected) {
                let label = if i.added_by == "list" { "Both of us".into() } else { i.added_by.clone() };
                p.push(Opt { value: i.added_by.clone(), label, selected: true });
            }
            p
        },
        pair_opts,
        me_name: me_name(s, jar),
        closed_opts: picker::WEEKDAYS
            .iter()
            .map(|(k, name)| Opt { value: (*k).into(), label: name[..3].into(), selected: i.closed.iter().any(|c| c == k) })
            .collect(),
        season_from_opts: month_opts(&i.season_from),
        season_to_opts: month_opts(&i.season_to),
        hours: i.hours.clone(),
        hours_today: match crate::hours::Hours::parse(&i.hours) {
            Some(h) => h.label(db::local_today(&s.db).await?.2),
            None => String::new(),
        },
        best_times: Opt::list(db::BEST_TIMES, std::slice::from_ref(&i.best_time)),
        minutes: i.minutes.clone(),
        bookings: Opt::list(db::BOOKINGS, std::slice::from_ref(&i.booking)),
        book_ahead_days: i.book_ahead_days.clone(),
        covers: Opt::list(db::COVERS, std::slice::from_ref(&i.cover)),
        cover_unknown: !db::COVERS.iter().any(|(k, _)| *k == i.cover),
        bring: i.bring.clone(),
        station: i.station.clone(),
        station_walk_min: i.station_walk_min.clone(),
        details_open: error.contains("Hours")
            || [&i.hours, &i.best_time, &i.minutes, &i.booking, &i.bring, &i.station].iter().any(|v| !v.trim().is_empty()),
    })
}

fn month_opts(selected: &str) -> Vec<Opt> {
    (1..=12)
        .map(|m: usize| Opt {
            value: m.to_string(),
            label: picker::MONTHS_SHORT[m - 1].into(),
            selected: selected == m.to_string(),
        })
        .collect()
}

fn input_from(s: &AppState, a: &Activity) -> ActivityInput {
    ActivityInput {
        title: s.cfg.personalize(&a.title),
        notes: s.cfg.personalize(&a.notes),
        est_cost: a.est_cost,
        setting: a.setting.clone(),
        duration: a.duration.clone(),
        venue: a.venue.clone(),
        area: a.area.clone(),
        location: a.latlng().map(|(lat, lng)| format!("{lat}, {lng}")).unwrap_or_default(),
        tags: a.tags.replace(',', ", "),
        energy: a.energy.clone(),
        added_by: a.added_by.clone(),
        pairs: vec![],
        closed: a.closed_days.split(',').filter(|d| !d.is_empty()).map(str::to_string).collect(),
        season_from: a.season().map(|s| s.0.to_string()).unwrap_or_default(),
        season_to: a.season().map(|s| s.1.to_string()).unwrap_or_default(),
        hours: a.hours.clone(),
        best_time: a.best_time.clone(),
        minutes: a.minutes.map(|m| m.to_string()).unwrap_or_default(),
        booking: a.booking.clone(),
        book_ahead_days: a.book_ahead_days.map(|m| m.to_string()).unwrap_or_default(),
        cover: a.cover.clone(),
        bring: a.bring.clone(),
        station: a.station.clone(),
        station_walk_min: a.station_walk_min.map(|m| m.to_string()).unwrap_or_default(),
    }
}

pub async fn new(State(s): State<AppState>, jar: SignedCookieJar) -> HtmlResult {
    let blank = ActivityInput {
        title: String::new(),
        notes: String::new(),
        est_cost: 0,
        setting: "local".into(),
        duration: "short".into(),
        venue: String::new(),
        area: String::new(),
        location: String::new(),
        tags: String::new(),
        energy: "med".into(),
        added_by: s.cfg.people[0].0.clone(),
        ..Default::default()
    };
    form(&s, &jar, None, &blank, "").await
}

pub async fn edit(State(s): State<AppState>, jar: SignedCookieJar, Path(id): Path<i64>) -> Result<Response, AppError> {
    Ok(match db::get(&s.db, id).await? {
        Some(a) => {
            let mut input = input_from(&s, &a);
            input.pairs = db::partners(&s.db, id).await?.iter().map(|p| p.id).collect();
            form(&s, &jar, Some(id), &input, "").await?.into_response()
        }
        None => Redirect::to("/wishlist").into_response(),
    })
}

async fn save(s: AppState, jar: SignedCookieJar, id: Option<i64>, i: ActivityInput) -> Result<Response, AppError> {
    let latlng = parse_latlng(&i.location);
    let error = if i.title.trim().is_empty() {
        "Give it a name."
    } else if !i.location.trim().is_empty() && latlng.is_none() {
        "Location should look like “19.13, 72.81”. Leave it blank for at-home plans."
    } else if !i.hours.trim().is_empty() && crate::hours::Hours::parse(&i.hours).is_none() {
        "Hours should look like “tue-sun 12:00-23:30”, with a “;” between different days."
    } else if Duration::parse(&i.duration).is_none() || !SETTINGS.iter().any(|(k, _)| *k == i.setting) {
        "Pick a setting and duration."
    } else {
        ""
    };
    if !error.is_empty() {
        return Ok(form(&s, &jar, id, &i, error).await?.into_response());
    }
    db::save(&s.db, id, &i, latlng).await?;
    Ok(Redirect::to("/wishlist").into_response())
}

pub async fn create(State(s): State<AppState>, jar: SignedCookieJar, Form(i): Form<ActivityInput>) -> Result<Response, AppError> {
    save(s, jar, None, i).await
}

pub async fn update(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    Path(id): Path<i64>,
    Form(i): Form<ActivityInput>,
) -> Result<Response, AppError> {
    save(s, jar, Some(id), i).await
}

pub async fn archive(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Redirect, AppError> {
    db::set_archived(&s.db, id, true).await?;
    Ok(Redirect::to("/wishlist"))
}

pub async fn restore(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Redirect, AppError> {
    db::set_archived(&s.db, id, false).await?;
    Ok(Redirect::to("/wishlist"))
}

/// Bring a slip someone passed on back into the draw.
pub async fn unveto(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Redirect, AppError> {
    db::unveto(&s.db, id).await?;
    Ok(Redirect::to("/wishlist"))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Redirect, AppError> {
    db::delete(&s.db, id).await?;
    Ok(Redirect::to("/wishlist"))
}

fn me_name(s: &AppState, jar: &SignedCookieJar) -> String {
    super::Me::new(&s.cfg, crate::auth::who(jar)).name
}
