pub mod auth;
pub mod decide;
pub mod history;
pub mod wishlist;

use askama::Template;
use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};

use crate::{config::Config, db::Activity, picker, AppState};

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
    /// Suggested by the app ("generated for you").
    pub generated: bool,
    /// Titles of slips this pairs with (filled in where shown).
    pub pairs: Vec<String>,
    /// One-way cab and train estimates from each of our homes (empty when we can't tell).
    pub travel: Vec<TravelLine>,
    /// Opens Uber with this place as the drop-off, to see the real fare.
    pub uber_url: Option<String>,
}

pub struct TravelLine {
    /// "Rohit"
    pub name: String,
    /// "~₹380"
    pub cab: String,
    /// "35 min"
    pub cab_time: String,
    pub train: String,
    pub train_time: String,
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
            generated: a.generated,
            pairs: vec![],
            travel: cfg
                .distances(a.latlng())
                .iter()
                .zip(&cfg.people)
                .filter_map(|(d, (name, _))| {
                    let km = (*d)?;
                    let (cab, train) = (crate::travel::cab(km), crate::travel::train(km));
                    Some(TravelLine {
                        name: name.clone(),
                        cab: format!("~{}", cfg.money(cab.rupees)),
                        cab_time: crate::travel::duration_label(cab.minutes),
                        train: format!("~{}", cfg.money(train.rupees)),
                        train_time: crate::travel::duration_label(train.minutes),
                    })
                })
                .collect(),
            uber_url: a.latlng().map(|(lat, lng)| {
                format!(
                    "https://m.uber.com/ul/?action=setPickup&pickup=my_location\
                     &dropoff%5Blatitude%5D={lat}&dropoff%5Blongitude%5D={lng}&dropoff%5Bnickname%5D={}",
                    url_encode(if a.venue.is_empty() { &a.title } else { &a.venue })
                )
            }),
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

/// Which of us is holding this device, with names for templates.
pub struct Me {
    pub who: Option<usize>,
    /// "Rohit", or empty when we don't know yet.
    pub name: String,
    /// The other one of us ("Neha"), or "your partner" when unknown.
    pub partner: String,
}

impl Me {
    pub fn new(cfg: &Config, who: Option<usize>) -> Self {
        Self {
            who,
            name: who.map(|i| cfg.people[i].0.clone()).unwrap_or_default(),
            partner: who.map(|i| cfg.people[1 - i].0.clone()).unwrap_or_else(|| "your partner".into()),
        }
    }
}

/// "₹4,200 of ₹6,000 this month".
pub struct BudgetView {
    pub spent: String,
    /// Empty when no monthly budget is set.
    pub budget: String,
    /// Raw number for the edit field ("6000" or "").
    pub budget_raw: String,
    /// 0–100, for a progress bar.
    pub pct: u8,
    pub over: bool,
}

pub async fn budget_view(s: &AppState) -> Result<BudgetView, AppError> {
    let spent = crate::db::spent_this_month(&s.db).await?;
    let budget: Option<i64> = crate::db::setting(&s.db, "monthly_budget").await?.and_then(|v| v.parse().ok());
    Ok(BudgetView {
        spent: if spent == 0 { format!("{}0", s.cfg.currency) } else { s.cfg.money(spent) },
        budget: budget.map(|b| s.cfg.money(b)).unwrap_or_default(),
        budget_raw: budget.map(|b| b.to_string()).unwrap_or_default(),
        pct: budget.filter(|b| *b > 0).map(|b| ((spent * 100) / b).clamp(0, 100) as u8).unwrap_or(0),
        over: budget.is_some_and(|b| spent > b),
    })
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MON: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "YYYY-MM-DD HH:MM W" (from SQLite) → "Sat 18 Oct, 7:00 pm".
pub fn planned_label(local: &str) -> String {
    let parse = || -> Option<String> {
        let (date, rest) = local.split_once(' ')?;
        let (hm, wd) = rest.split_once(' ')?;
        let mut d = date.split('-').skip(1);
        let (m, day): (usize, u32) = (d.next()?.parse().ok()?, d.next()?.parse().ok()?);
        let (h, min): (u32, u32) = {
            let (h, m) = hm.split_once(':')?;
            (h.parse().ok()?, m.parse().ok()?)
        };
        let (h12, ampm) = match h {
            0 => (12, "am"),
            1..=11 => (h, "am"),
            12 => (12, "pm"),
            _ => (h - 12, "pm"),
        };
        Some(format!("{} {day} {}, {h12}:{min:02} {ampm}", WEEKDAYS.get(wd.parse::<usize>().ok()?)?, MON.get(m.checked_sub(1)?)?))
    };
    parse().unwrap_or_default()
}

/// "YYYY-MM-DD HH:MM W" → the value a `<input type="datetime-local">` expects.
pub fn planned_input(local: &str) -> String {
    local.get(..16).map(|s| s.replacen(' ', "T", 1)).unwrap_or_default()
}

/// Minimal percent-encoding for a URL query value.
pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// What we send each other on WhatsApp about a slip.
pub fn share_text(v: &ActivityView) -> String {
    let mut t = format!("Date idea from our jar: {}", v.title);
    if !v.place.is_empty() {
        t += &format!(" ({})", v.place);
    }
    t += &format!(" · {} · {}", v.cost, v.duration);
    if let Some(url) = &v.maps_url {
        t += &format!("\n{url}");
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_planned_times() {
        assert_eq!(planned_label("2026-10-18 19:00 0"), "Sun 18 Oct, 7:00 pm");
        assert_eq!(planned_label("2026-01-03 00:05 6"), "Sat 3 Jan, 12:05 am");
        assert_eq!(planned_label("garbage"), "");
        assert_eq!(planned_input("2026-10-18 19:00 0"), "2026-10-18T19:00");
    }

    #[test]
    fn encodes_for_whatsapp_links() {
        assert_eq!(url_encode("Cat café & chai\n"), "Cat%20caf%C3%A9%20%26%20chai%0A");
    }
}
