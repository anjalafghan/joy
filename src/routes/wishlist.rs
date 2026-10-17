use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{Form, SignedCookieJar};

use super::{duration_opts, render, ActivityView, AppError, HtmlResult, Opt};
use crate::{
    config::parse_latlng,
    db::{self, Activity, ActivityInput},
    picker::{Duration, ENERGIES, SETTINGS},
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
    let view = |a: &Activity| {
        let mut v = ActivityView::new(&s.cfg, a);
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
    let archived = db::list(&s.db, true).await?.iter().map(|a| ActivityView::new(&s.cfg, a)).collect();
    render(WishlistTpl { me_name: me_name(&s, &jar), groups, archived, total: active.len() })
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
        settings: Opt::list(SETTINGS, &[i.setting.clone()]),
        durations: duration_opts(&i.duration),
        energies: Opt::list(ENERGIES, &[i.energy.clone()]),
        people: {
            let mut p = Opt::list(people, &[i.added_by.clone()]);
            // Keep an author we don't know (e.g. imported "list") instead of silently overwriting it.
            if !i.added_by.is_empty() && !p.iter().any(|o| o.selected) {
                let label = if i.added_by == "list" { "Both of us".into() } else { i.added_by.clone() };
                p.push(Opt { value: i.added_by.clone(), label, selected: true });
            }
            p
        },
        pair_opts,
        me_name: me_name(s, jar),
    })
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
        pairs: vec![],
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

pub async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Redirect, AppError> {
    db::delete(&s.db, id).await?;
    Ok(Redirect::to("/wishlist"))
}

fn me_name(s: &AppState, jar: &SignedCookieJar) -> String {
    super::Me::new(&s.cfg, crate::auth::who(jar)).name
}
