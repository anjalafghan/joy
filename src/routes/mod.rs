pub mod auth;
pub mod decide;
pub mod history;
pub mod middle;
pub mod wishlist;
pub mod year;

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
    /// home | apart | local | day_trip | travel — for per-setting colour.
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
    /// "calibrated from 4 real rides", when cab estimates have learned from us.
    pub travel_note: String,
    /// Why `cost` differs from the slip's own estimate, if it does.
    pub cost_note: String,
    /// "Closed Mondays · Nov–May only", or empty.
    pub closed_label: String,
    /// Who passed on this slip ("Neha"), when someone did (filled in where shown).
    pub vetoed_by: String,
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
    pub fn new(cfg: &Config, a: &Activity, learned: &Learned) -> Self {
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
            cost: cfg.money(a.effective_cost(learned.cost)),
            cost_note: match learned.cost {
                Some(_) if !a.est_from_actual && a.effective_cost(learned.cost) != a.est_cost => {
                    format!("slip says {}; adjusted for how your dates usually go", cfg.money(a.est_cost))
                }
                _ if a.est_from_actual => "from what it really cost last time".into(),
                _ => String::new(),
            },
            closed_label: closed_label(a),
            duration: a.duration().label().into(),
            setting: picker::setting_label(&a.setting).into(),
            setting_key: a.setting.clone(),
            tags: a.tag_list(),
            distances,
            maps_url: a.latlng().map(|(lat, lng)| format!("https://www.google.com/maps/search/?api=1&query={lat},{lng}")),
            times_done: a.times_done,
            generated: a.generated,
            pairs: vec![],
            vetoed_by: String::new(),
            travel_note: learned.cab.map(|_| format!("calibrated from {} real ride{}", learned.rides, if learned.rides == 1 { "" } else { "s" })).unwrap_or_default(),
            travel: cfg
                .distances(a.latlng())
                .iter()
                .zip(&cfg.people)
                .filter_map(|(d, (name, _))| {
                    let km = (*d)?;
                    let mut cab = crate::travel::cab(km);
                    if let Some(f) = learned.cab {
                        cab.rupees = ((cab.rupees as f64 * f) / 10.0).round() as i64 * 10;
                    }
                    let train = crate::travel::train(km);
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
    /// This month's budget, including anything rolled over. Empty when none is set.
    pub budget: String,
    /// Raw standing amount for the edit field ("6000" or "").
    pub budget_raw: String,
    /// 0–100, for a progress bar.
    pub pct: u8,
    pub over: bool,
    /// "includes ₹1,800 from last month", or empty.
    pub carry_note: String,
    /// Last month's leftover, offered until we choose to roll it over or start fresh.
    pub rollover: Option<Rollover>,
}

pub struct Rollover {
    /// "October".
    pub month: String,
    pub spent: String,
    pub budget: String,
    pub left: String,
}

/// A finished month in the "Past months" list.
pub struct PastMonth {
    pub label: String,
    pub spent: String,
    /// Empty when no budget was set.
    pub budget: String,
}

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// "2026-10" → "October" (plus the year when it isn't this one).
fn month_name(ym: &str, this_year: &str) -> String {
    let (y, m) = ym.split_once('-').unwrap_or_default();
    let name = m.parse::<usize>().ok().and_then(|m| MONTHS.get(m.wrapping_sub(1))).copied().unwrap_or_default();
    if y == this_year { name.into() } else { format!("{name} {y}") }
}

fn standing_budget(v: Option<String>) -> Option<i64> {
    v.and_then(|v| v.parse().ok()).filter(|b| *b > 0)
}

/// What last month left over, if it's still ours to roll over: not yet decided this month,
/// a budget is set, and last month came in under it.
async fn pending_rollover(s: &AppState, standing: Option<i64>) -> Result<Option<(String, i64, i64, i64)>, AppError> {
    let Some(standing) = standing else { return Ok(None) };
    let (this, prev) = (crate::db::month_offset(&s.db, 0).await?, crate::db::month_offset(&s.db, 1).await?);
    if crate::db::month_budget(&s.db, &this).await?.is_some() {
        return Ok(None);
    }
    let spent = crate::db::spent_in_month(&s.db, &prev).await?;
    let prev_row = crate::db::month_budget(&s.db, &prev).await?;
    if spent == 0 && prev_row.is_none() {
        return Ok(None);
    }
    let budget = prev_row.map(|(b, c)| b + c).unwrap_or(standing);
    Ok(Some((prev, spent, budget, (budget - spent).max(0))).filter(|r| r.3 > 0))
}

pub async fn budget_view(s: &AppState) -> Result<BudgetView, AppError> {
    let spent = crate::db::spent_this_month(&s.db).await?;
    let standing = standing_budget(crate::db::setting(&s.db, "monthly_budget").await?);
    let this = crate::db::month_offset(&s.db, 0).await?;
    let carry = crate::db::month_budget(&s.db, &this).await?.map(|(_, c)| c).unwrap_or(0);
    let budget = standing.map(|b| b + carry);
    let year = this.get(..4).unwrap_or_default();
    let rollover = pending_rollover(s, standing).await?.map(|(prev, spent, budget, left)| Rollover {
        month: month_name(&prev, year),
        spent: s.cfg.money(spent),
        budget: s.cfg.money(budget),
        left: s.cfg.money(left),
    });
    Ok(BudgetView {
        spent: if spent == 0 { format!("{}0", s.cfg.currency) } else { s.cfg.money(spent) },
        budget: budget.map(|b| s.cfg.money(b)).unwrap_or_default(),
        budget_raw: standing.map(|b| b.to_string()).unwrap_or_default(),
        pct: budget.map(|b| ((spent * 100) / b).clamp(0, 100) as u8).unwrap_or(0),
        over: budget.is_some_and(|b| spent > b),
        carry_note: if carry > 0 && standing.is_some() { format!("includes {} rolled over", s.cfg.money(carry)) } else { String::new() },
        rollover,
    })
}

/// Finished months, newest first, each against the budget it had (or today's, if we never decided).
pub async fn past_months(s: &AppState) -> Result<Vec<PastMonth>, AppError> {
    let standing = standing_budget(crate::db::setting(&s.db, "monthly_budget").await?);
    let this = crate::db::month_offset(&s.db, 0).await?;
    let year = this.get(..4).unwrap_or_default();
    let mut out = vec![];
    for (month, spent) in crate::db::past_months(&s.db).await? {
        let budget = crate::db::month_budget(&s.db, &month).await?.map(|(b, c)| b + c).or(standing);
        out.push(PastMonth {
            label: month_name(&month, year),
            spent: s.cfg.money(spent),
            budget: budget.map(|b| s.cfg.money(b)).unwrap_or_default(),
        });
    }
    Ok(out)
}

/// Roll last month's leftover over (or start fresh) for this month. Does nothing if already decided.
pub async fn settle_month(s: &AppState, roll: bool) -> Result<(), AppError> {
    let standing = standing_budget(crate::db::setting(&s.db, "monthly_budget").await?);
    let Some(base) = standing else { return Ok(()) };
    let this = crate::db::month_offset(&s.db, 0).await?;
    let carry = match pending_rollover(s, standing).await? {
        Some((_, _, _, left)) if roll => left,
        _ => 0,
    };
    crate::db::decide_month(&s.db, &this, base, carry).await?;
    Ok(())
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

/// What the jar has learned from real costs. Worked out per request (a couple of tiny queries).
#[derive(Debug, Clone, Default)]
pub struct Learned {
    /// Multiply plan estimates by this (None until there are enough real costs).
    pub cost: Option<f64>,
    /// Real ÷ estimated over all dates with a real cost, and how many there are.
    pub cost_ratio: f64,
    pub cost_samples: i64,
    /// Multiply cab estimates by this (None until there are enough real rides).
    pub cab: Option<f64>,
    pub rides: i64,
}

/// Learned factors are clamped so one odd night can't wreck every estimate.
const FACTOR_RANGE: (f64, f64) = (0.5, 3.0);

pub async fn learned(s: &AppState) -> Result<Learned, AppError> {
    let (ratio, n) = crate::db::cost_ratio(&s.db).await?;
    let (cab_ratio, rides) = crate::db::cab_ratio(&s.db).await?;
    let usable = |r: f64, n: i64| (n >= crate::db::MIN_SAMPLES).then(|| r.clamp(FACTOR_RANGE.0, FACTOR_RANGE.1));
    Ok(Learned { cost: usable(ratio, n), cost_ratio: ratio, cost_samples: n, cab: usable(cab_ratio, rides), rides })
}

/// "Closed Mondays & Wednesdays · Nov–May only".
fn closed_label(a: &Activity) -> String {
    let days: Vec<&str> = picker::parse_closed(&a.closed_days)
        .into_iter()
        .filter_map(|d| picker::WEEKDAYS.get(d as usize).map(|(_, n)| *n))
        .collect();
    let mut parts = vec![];
    if !days.is_empty() {
        let days: Vec<String> = days.iter().map(|d| format!("{d}s")).collect();
        let list = match days.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} & {last}", rest.join(", ")),
            _ => days.join(""),
        };
        parts.push(format!("Closed {list}"));
    }
    if let Some((from, to)) = a.season() {
        parts.push(format!("{} only", picker::season_label(from, to)));
    }
    parts.join(" · ")
}
