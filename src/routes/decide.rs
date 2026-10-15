use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::{
    extract::{Path, State},
    Json,
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};

use super::{distance_from, duration_opts, render, ActivityView, AppError, HtmlResult, Opt};
use crate::{
    db,
    picker::{self, Candidate, Duration, Filters, Hint, PickResult, ENERGIES, MAX_REROLLS, SETTINGS},
    AppState,
};

const KM_CHOICES: [(&str, &str); 5] =
    [("15", "15 km"), ("30", "30 km"), ("45", "45 km"), ("60", "60 km"), ("100", "100 km")];

/// One slip in the jar, as the page's script sees it.
#[derive(Serialize)]
struct Slip<'a> {
    id: i64,
    title: String,
    setting: &'a str,
}

#[derive(Template)]
#[template(path = "decide.html")]
struct DecideTpl {
    couple: String,
    currency: String,
    count: usize,
    /// JSON array of `Slip`, safe to drop inside <script type="application/json">.
    slips_json: String,
    durations: Vec<Opt>,
    settings: Vec<Opt>,
    tags: Vec<Opt>,
    energies: Vec<Opt>,
    kms: Vec<Opt>,
    froms: Vec<Opt>,
}

pub async fn page(State(s): State<AppState>) -> HtmlResult {
    let acts = db::list(&s.db, false).await?;
    let mut tags: Vec<String> = acts.iter().flat_map(|a| a.tag_list()).collect();
    tags.sort();
    tags.dedup();
    let slips: Vec<Slip> =
        acts.iter().map(|a| Slip { id: a.id, title: s.cfg.personalize(&a.title), setting: &a.setting }).collect();
    render(DecideTpl {
        couple: format!("{} & {}", s.cfg.people[0].0, s.cfg.people[1].0),
        currency: s.cfg.currency.clone(),
        count: acts.len(),
        slips_json: script_safe_json(&slips)?,
        durations: duration_opts(""),
        settings: Opt::list(SETTINGS, &[]),
        tags: tags.iter().map(|t| Opt { value: t.clone(), label: t.clone(), selected: false }).collect(),
        energies: Opt::list(ENERGIES, &[]),
        kms: Opt::list(KM_CHOICES, &[]),
        froms: s
            .cfg
            .people
            .iter()
            .enumerate()
            .filter(|(_, (_, home))| home.is_some())
            .map(|(i, (name, _))| Opt { value: i.to_string(), label: format!("{name}'s place"), selected: false })
            .collect(),
    })
}

fn script_safe_json(v: &impl Serialize) -> Result<String, AppError> {
    Ok(serde_json::to_string(v)?.replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026"))
}

/// Everything the pick card round-trips in hidden fields, so rerolls are stateless.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct PickForm {
    budget: String,
    max_duration: String,
    settings: Vec<String>,
    tag: String,
    energy: String,
    max_km: String,
    /// "" = whoever's farther, "0"/"1" = that person's home.
    from: String,
    excluded: String,
    used: String,
    current: String,
    reroll: String,
}

impl PickForm {
    fn filters(&self) -> Filters {
        let opt = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        Filters {
            budget: self.budget.trim().parse().unwrap_or(i64::MAX),
            max_duration: Duration::parse(&self.max_duration),
            settings: self.settings.iter().filter(|s| !s.is_empty()).cloned().collect(),
            tag: opt(&self.tag),
            energy: opt(&self.energy),
            max_km: self.max_km.trim().parse().ok(),
        }
    }

    fn from(&self) -> Option<usize> {
        self.from.parse().ok().filter(|i| *i < 2)
    }

    fn excluded(&self) -> Vec<i64> {
        self.excluded.split(',').filter_map(|s| s.trim().parse().ok()).collect()
    }

    /// The filter fields only, as hidden inputs.
    fn filter_fields(&self) -> Vec<(String, String)> {
        let mut h: Vec<(String, String)> = vec![
            ("budget".into(), self.budget.clone()),
            ("max_duration".into(), self.max_duration.clone()),
            ("tag".into(), self.tag.clone()),
            ("energy".into(), self.energy.clone()),
            ("max_km".into(), self.max_km.clone()),
            ("from".into(), self.from.clone()),
        ];
        h.extend(self.settings.iter().map(|s| ("settings".into(), s.clone())));
        h
    }

    fn hidden(&self, excluded: &[i64], used: u32, current: i64) -> Vec<(String, String)> {
        let mut h = self.filter_fields();
        h.push(("excluded".into(), excluded.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")));
        h.push(("used".into(), used.to_string()));
        h.push(("current".into(), current.to_string()));
        h
    }
}

async fn candidates(s: &AppState, f: &PickForm) -> Result<(Vec<db::Activity>, Vec<Candidate>), AppError> {
    let acts = db::list(&s.db, false).await?;
    let cands = acts
        .iter()
        .map(|a| {
            let mut c = a.candidate(distance_from(&s.cfg, a, f.from()));
            c.title = s.cfg.personalize(&c.title);
            c
        })
        .collect();
    Ok((acts, cands))
}

#[derive(Serialize)]
pub struct MatchCount {
    count: usize,
    total: usize,
    ids: Vec<i64>,
}

/// Which slips fit the current filters — lets the jar react live as filters change.
pub async fn matching(State(s): State<AppState>, Form(f): Form<PickForm>) -> Result<Json<MatchCount>, AppError> {
    let (_, cands) = candidates(&s, &f).await?;
    let filters = f.filters();
    let ids: Vec<i64> = cands.iter().filter(|c| picker::matches(c, &filters)).map(|c| c.id).collect();
    Ok(Json(MatchCount { count: ids.len(), total: cands.len(), ids }))
}

#[derive(Template)]
#[template(path = "pick_card.html")]
struct CardTpl {
    activity: Option<ActivityView>,
    /// Shown when nothing fits (empty when there is a pick).
    message: String,
    /// One-tap fix for the no-match case: button label + the loosened filters.
    relax_label: String,
    relax_hidden: Vec<(String, String)>,
    done: bool,
    /// For the stamp on an accepted pick, e.g. "14 · 10 · 2026".
    stamp_date: String,
    rerolls_left: u32,
    max_rerolls: u32,
    hidden: Vec<(String, String)>,
}

impl CardTpl {
    fn blank() -> Self {
        Self {
            activity: None,
            message: String::new(),
            relax_label: String::new(),
            relax_hidden: vec![],
            done: false,
            stamp_date: String::new(),
            rerolls_left: 0,
            max_rerolls: MAX_REROLLS,
            hidden: vec![],
        }
    }
}

pub async fn pick(State(s): State<AppState>, Form(f): Form<PickForm>) -> HtmlResult {
    let mut excluded = f.excluded();
    let mut used: u32 = f.used.parse().unwrap_or(0);

    if f.reroll == "1" {
        if let Ok(cur) = f.current.parse::<i64>() {
            if used >= MAX_REROLLS {
                // Out of rerolls: the card already hides the button, so just show it again.
                if let Some(a) = db::get(&s.db, cur).await? {
                    return card(&s, &f, Some(&a), &excluded, used);
                }
            }
            db::record(&s.db, cur, false).await?;
            excluded.push(cur);
            used += 1;
        }
    }

    let (acts, cands) = candidates(&s, &f).await?;
    let filters = f.filters();
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let result = picker::pick(&cands, &filters, &excluded, now, &mut rand::thread_rng());

    match result {
        PickResult::Pick(id) => card(&s, &f, acts.iter().find(|a| a.id == id), &excluded, used),
        PickResult::NoMatch(h) => {
            let (relax_label, relax) = relax(&s, &f, &h);
            render(CardTpl {
                message: hint_text(&s, &f, &filters, h),
                relax_label,
                relax_hidden: relax.map(|r| r.filter_fields()).unwrap_or_default(),
                ..CardTpl::blank()
            })
        }
    }
}

fn card(s: &AppState, f: &PickForm, a: Option<&db::Activity>, excluded: &[i64], used: u32) -> HtmlResult {
    render(CardTpl {
        hidden: f.hidden(excluded, used, a.map_or(0, |a| a.id)),
        activity: a.map(|a| ActivityView::new(&s.cfg, a)),
        rerolls_left: picker::rerolls_left(used),
        ..CardTpl::blank()
    })
}

fn hint_text(s: &AppState, form: &PickForm, f: &Filters, h: Hint) -> String {
    match h {
        Hint::EmptyWishlist => "The jar is empty. Write the first slip.".into(),
        Hint::Exhausted => "You've peeked at every slip that fits. Loosen something, or go with one you put back.".into(),
        Hint::OverBudget { title, cost } if f.budget == 0 => {
            format!("Nothing in the jar is free. Closest: {title} at {}.", s.cfg.money(cost))
        }
        Hint::OverBudget { title, cost } => format!(
            "Nothing in the jar fits under {}. Closest: {title} at {}.",
            s.cfg.money(f.budget),
            s.cfg.money(cost)
        ),
        Hint::NeedsMoreTime { title, duration } => {
            format!("Nothing fits that time. {title} needs {}.", duration.label().to_lowercase())
        }
        Hint::TooFar { title, km } => format!(
            "Nothing within {} km {}. Nearest: {title} (~{km} km).",
            f.max_km.unwrap_or_default(),
            match form.from() {
                Some(i) => format!("of {}'s place", s.cfg.people[i].0),
                None => "for both of you".into(),
            }
        ),
        Hint::OtherSetting { title, setting } => format!(
            "Nothing in those places. {title} ({}) would fit.",
            picker::setting_label(&setting).to_lowercase()
        ),
        Hint::NoMoodMatch => "Nothing matches that vibe or energy.".into(),
    }
}

/// The smallest change to the filters that would make the hinted slip fit.
fn relax(s: &AppState, f: &PickForm, h: &Hint) -> (String, Option<PickForm>) {
    let mut r = PickForm { excluded: String::new(), used: String::new(), current: String::new(), reroll: String::new(), ..f.clone() };
    let label = match h {
        Hint::OverBudget { cost, .. } => {
            r.budget = cost.to_string();
            format!("Stretch to {}", s.cfg.money(*cost))
        }
        Hint::NeedsMoreTime { duration, .. } => {
            r.max_duration = duration.key().into();
            format!("Make it {}", duration.label().to_lowercase())
        }
        Hint::TooFar { km, .. } => {
            let km = (km + 4) / 5 * 5;
            r.max_km = km.to_string();
            format!("Go up to {km} km")
        }
        Hint::OtherSetting { setting, .. } => {
            r.settings.push(setting.clone());
            format!("Include {}", picker::setting_label(setting).to_lowercase())
        }
        Hint::NoMoodMatch => {
            r.tag.clear();
            r.energy.clear();
            "Any vibe, any energy".into()
        }
        Hint::Exhausted => {
            return ("Start over with these filters".into(), Some(r));
        }
        Hint::EmptyWishlist => return (String::new(), None),
    };
    (label, Some(r))
}

pub async fn accept(State(s): State<AppState>, Path(id): Path<i64>) -> HtmlResult {
    db::accept(&s.db, id).await?;
    let a = db::get(&s.db, id).await?;
    render(CardTpl {
        activity: a.as_ref().map(|a| ActivityView::new(&s.cfg, a)),
        done: true,
        stamp_date: db::today(&s.db).await?,
        ..CardTpl::blank()
    })
}
