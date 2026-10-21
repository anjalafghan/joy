//! "Meet in the middle": each of us tells the jar, privately, how tonight feels. Once we
//! both have, the jar draws only from what fits both of us. Neither of us ever sees the
//! other's answers: not on the page, not in the pick card's hidden fields, not in a
//! "nothing fits under ₹500" hint. Answers last for the local day.

use axum::{extract::State, response::Redirect};
use axum_extra::extract::{Form, SignedCookieJar};
use serde::Deserialize;
use sqlx::SqlitePool;

use super::{AppError, Opt};
use crate::{auth, db, picker::Duration, AppState};

/// How far we'll go tonight, nearest first.
pub const REACHES: [(&str, &str); 4] =
    [("home", "Stay in"), ("near", "Nearby"), ("town", "Anywhere in town"), ("far", "Anywhere at all")];
/// Spend for both of us. None = any amount.
pub const BUDGETS: [Option<i64>; 5] = [Some(0), Some(500), Some(1500), Some(3000), None];
pub const ENERGY_WORDS: [(&str, &str); 3] = [("low", "Low"), ("med", "Some"), ("high", "Lots")];
pub const TIMES: [(&str, &str); 5] =
    [("short", "A couple of hours"), ("half_day", "Half a day"), ("full_day", "A whole day"), ("weekend", "The weekend"), ("", "Any")];
/// Vibes worth asking about (a subset of the tags slips use).
pub const VIBES: [&str; 8] = ["romantic", "cozy", "foodie", "chill", "adventurous", "cultural", "games", "outdoors"];

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Checkin {
    pub who: i64,
    pub energy: String,
    pub budget: Option<i64>,
    pub max_duration: String,
    pub reach: String,
    pub vibe: String,
}

fn energy_rank(e: &str) -> u8 {
    match e {
        "low" => 0,
        "med" => 1,
        _ => 2,
    }
}

fn reach_rank(r: &str) -> usize {
    REACHES.iter().position(|(k, _)| *k == r).unwrap_or(REACHES.len() - 1)
}

/// Where two check-ins meet: the tighter of each answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Middle {
    pub budget: Option<i64>,
    pub max_duration: Option<Duration>,
    /// Only at-home slips.
    pub stay_in: bool,
    pub max_km: Option<f64>,
    /// Highest energy either of us is up for ("low" if one of us is tired).
    pub max_energy: u8,
    /// Slips should match at least one of these (empty = any). When we picked the same vibe
    /// it's just that one.
    pub vibes: Vec<String>,
}

pub fn meet(a: &Checkin, b: &Checkin) -> Middle {
    let budget = match (a.budget, b.budget) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    let max_duration = match (Duration::parse(&a.max_duration), Duration::parse(&b.max_duration)) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    let reach = REACHES[reach_rank(&a.reach).min(reach_rank(&b.reach))].0;
    let mut vibes: Vec<String> = [&a.vibe, &b.vibe].into_iter().filter(|v| !v.is_empty()).cloned().collect();
    vibes.dedup();
    Middle {
        budget,
        max_duration,
        stay_in: reach == "home",
        max_km: match reach {
            "near" => Some(15.0),
            "town" => Some(45.0),
            _ => None,
        },
        max_energy: energy_rank(&a.energy).min(energy_rank(&b.energy)),
        vibes,
    }
}

impl Middle {
    /// Energy and vibe rules the picker's own filters can't express.
    pub fn allows(&self, energy: &str, tags: &[String]) -> bool {
        energy_rank(energy) <= self.max_energy && (self.vibes.is_empty() || tags.iter().any(|t| self.vibes.contains(t)))
    }
}

pub async fn today(db: &SqlitePool) -> sqlx::Result<Vec<Checkin>> {
    sqlx::query_as(
        "SELECT who, energy, budget, max_duration, reach, vibe FROM checkins WHERE day = date('now', 'localtime') ORDER BY who",
    )
    .fetch_all(db)
    .await
}

/// Both of tonight's check-ins, met in the middle, or None until we both have one.
pub async fn tonight(db: &SqlitePool) -> sqlx::Result<Option<Middle>> {
    let c = today(db).await?;
    Ok(match c.as_slice() {
        [a, b] => Some(meet(a, b)),
        _ => None,
    })
}

/// What the jar page shows, from this viewer's side. Never carries the partner's answers.
pub struct MiddleView {
    /// The jar knows who's holding the phone (the feature needs that).
    pub enabled: bool,
    pub mine: Option<Checkin>,
    pub partner_in: bool,
    pub partner: String,
    pub energies: Vec<Opt>,
    pub budgets: Vec<Opt>,
    pub times: Vec<Opt>,
    pub reaches: Vec<Opt>,
    pub vibes: Vec<Opt>,
    /// WhatsApp link asking the partner to check in.
    pub wa_url: String,
}

impl MiddleView {
    pub fn both_in(&self) -> bool {
        self.mine.is_some() && self.partner_in
    }

    pub fn no_vibe(&self) -> bool {
        !self.vibes.iter().any(|v| v.selected)
    }
}

pub async fn view(s: &AppState, who: Option<usize>, partner: &str) -> Result<MiddleView, AppError> {
    let all = today(&s.db).await?;
    let mine = who.and_then(|w| all.iter().find(|c| c.who == w as i64).cloned());
    let partner_in = who.is_some_and(|w| all.iter().any(|c| c.who != w as i64));
    let pick = |list: &[(&str, &str)], current: &str| -> Vec<Opt> {
        list.iter().map(|(v, l)| Opt { value: (*v).into(), label: (*l).into(), selected: *v == current }).collect()
    };
    let m = mine.as_ref();
    let budget = m.map(|c| c.budget.map(|b| b.to_string()).unwrap_or_default()).unwrap_or_else(|| "1500".into());
    let msg = "I've told our jar how tonight feels. Your turn: open joy and check in, then we'll draw from the middle.";
    Ok(MiddleView {
        enabled: who.is_some(),
        partner_in,
        partner: partner.into(),
        energies: pick(&ENERGY_WORDS, m.map_or("med", |c| c.energy.as_str())),
        budgets: BUDGETS
            .iter()
            .map(|b| {
                let value = b.map(|n| n.to_string()).unwrap_or_default();
                let label = match b {
                    Some(0) => "Free".into(),
                    Some(n) => s.cfg.money(*n),
                    None => "Any".into(),
                };
                Opt { selected: value == budget, value, label }
            })
            .collect(),
        times: pick(&TIMES, m.map_or("short", |c| c.max_duration.as_str())),
        reaches: pick(&REACHES, m.map_or("town", |c| c.reach.as_str())),
        vibes: VIBES
            .iter()
            .map(|v| Opt { value: (*v).into(), label: (*v).into(), selected: m.is_some_and(|c| c.vibe == *v) })
            .collect(),
        wa_url: format!("https://wa.me/?text={}", super::url_encode(msg)),
        mine,
    })
}

#[derive(Deserialize)]
pub struct CheckinForm {
    energy: String,
    #[serde(default)]
    budget: String,
    #[serde(default)]
    max_duration: String,
    #[serde(default)]
    reach: String,
    #[serde(default)]
    vibe: String,
    /// "1": take my check-in back.
    #[serde(default)]
    clear: String,
}

pub async fn check_in(State(s): State<AppState>, jar: SignedCookieJar, Form(f): Form<CheckinForm>) -> Result<Redirect, AppError> {
    let Some(who) = auth::who(&jar) else { return Ok(Redirect::to("/who")) };
    let (day, ..) = db::local_today(&s.db).await?;
    sqlx::query("DELETE FROM checkins WHERE day < ? OR (who = ? AND ? = '1')").bind(&day).bind(who as i64).bind(&f.clear).execute(&s.db).await?;
    if f.clear != "1" {
        let energy = if ENERGY_WORDS.iter().any(|(k, _)| *k == f.energy) { f.energy } else { "med".into() };
        let budget: Option<i64> = f.budget.trim().parse().ok().filter(|b: &i64| *b >= 0);
        let max_duration = Duration::parse(&f.max_duration).map(|d| d.key().to_string()).unwrap_or_default();
        let reach = REACHES.iter().find(|(k, _)| *k == f.reach).map_or("far", |(k, _)| k);
        let vibe = VIBES.iter().find(|v| **v == f.vibe).map_or("", |v| v);
        sqlx::query(
            "INSERT INTO checkins (who, day, energy, budget, max_duration, reach, vibe) VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (who, day) DO UPDATE SET energy = excluded.energy, budget = excluded.budget,
               max_duration = excluded.max_duration, reach = excluded.reach, vibe = excluded.vibe, at = unixepoch()",
        )
        .bind(who as i64)
        .bind(&day)
        .bind(energy)
        .bind(budget)
        .bind(max_duration)
        .bind(reach)
        .bind(vibe)
        .execute(&s.db)
        .await?;
    }
    Ok(Redirect::to("/#middle"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(energy: &str, budget: Option<i64>, time: &str, reach: &str, vibe: &str) -> Checkin {
        Checkin { who: 0, energy: energy.into(), budget, max_duration: time.into(), reach: reach.into(), vibe: vibe.into() }
    }

    #[test]
    fn the_tighter_answer_wins_each_time() {
        let m = meet(&c("high", Some(3000), "full_day", "far", "adventurous"), &c("low", Some(1500), "short", "near", "cozy"));
        assert_eq!(m.budget, Some(1500));
        assert_eq!(m.max_duration, Some(Duration::Short));
        assert_eq!(m.max_km, Some(15.0));
        assert!(!m.stay_in);
        assert_eq!(m.max_energy, 0, "one of us is tired, so nothing high-energy");
        assert_eq!(m.vibes, vec!["adventurous".to_string(), "cozy".to_string()]);
    }

    #[test]
    fn any_defers_to_the_other_and_stay_in_wins() {
        let m = meet(&c("med", None, "", "home", ""), &c("med", Some(500), "half_day", "far", "cozy"));
        assert_eq!(m.budget, Some(500));
        assert_eq!(m.max_duration, Some(Duration::HalfDay));
        assert!(m.stay_in);
        assert_eq!(m.vibes, vec!["cozy".to_string()]);
        assert!(m.allows("low", &["cozy".into()]));
        assert!(!m.allows("high", &["cozy".into()]));
        assert!(!m.allows("low", &["foodie".into()]));
    }
}
