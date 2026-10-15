pub mod auth;
pub mod decide;
pub mod history;
pub mod wishlist;

use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use crate::{config::Config, db::Activity, picker};

pub struct AppError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        Self(e.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("{:#}", self.0);
        (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong. Check the server log.").into_response()
    }
}

pub type HtmlResult = Result<Html<String>, AppError>;

pub fn render(t: impl Template) -> HtmlResult {
    Ok(Html(t.render()?))
}

/// A <select>/<checkbox> option with its selected state precomputed.
pub struct Opt {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

impl Opt {
    pub fn list<'a>(items: impl IntoIterator<Item = (&'a str, &'a str)>, selected: &[String]) -> Vec<Opt> {
        items
            .into_iter()
            .map(|(v, l)| Opt { value: v.into(), label: l.into(), selected: selected.iter().any(|s| s == v) })
            .collect()
    }
}

pub fn duration_opts(selected: &str) -> Vec<Opt> {
    Opt::list(picker::Duration::ALL.iter().map(|d| (d.key(), d.label())), &[selected.into()])
}

/// An activity ready for display.
pub struct ActivityView {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub place: String,
    pub cost: String,
    pub duration: String,
    pub setting: String,
    /// home | local | day_trip | travel — for per-setting colour.
    pub setting_key: String,
    pub tags: Vec<String>,
    pub distances: Vec<String>,
    pub maps_url: Option<String>,
    pub times_done: i64,
}

impl ActivityView {
    pub fn new(cfg: &Config, a: &Activity) -> Self {
        let distances = cfg
            .distances(a.latlng())
            .iter()
            .zip(&cfg.people)
            .filter_map(|(d, (name, _))| d.map(|km| format!("{name} ~{} km", km.round())))
            .collect();
        let place = [a.venue.as_str(), a.area.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
        Self {
            id: a.id,
            title: cfg.personalize(&a.title),
            notes: cfg.personalize(&a.notes),
            place,
            cost: cfg.money(a.est_cost),
            duration: a.duration().label().into(),
            setting: picker::setting_label(&a.setting).into(),
            setting_key: a.setting.clone(),
            tags: a.tag_list(),
            distances,
            maps_url: a.latlng().map(|(lat, lng)| format!("https://www.google.com/maps/search/?api=1&query={lat},{lng}")),
            times_done: a.times_done,
        }
    }
}

/// Distance the "how far" filter compares against: from one person's home,
/// or (`None`) whichever of us is farther.
pub fn distance_from(cfg: &Config, a: &Activity, from: Option<usize>) -> Option<f64> {
    let d = cfg.distances(a.latlng());
    match from {
        Some(i) => d.get(i).copied().flatten(),
        None => d.into_iter().flatten().reduce(f64::max),
    }
}
