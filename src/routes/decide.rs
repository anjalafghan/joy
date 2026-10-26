use std::time::{SystemTime, UNIX_EPOCH};

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};

use axum_extra::extract::SignedCookieJar;

use super::{
    learned, Learned,
    budget_view, distance_from, duration_opts, render, share_text, url_encode, ActivityView, AppError, BudgetView,
    HtmlResult, Me, Opt,
};
use crate::{
    db,
    auth,
    routes::middle::Middle,
    picker::{self, Candidate, Duration, Filters, Hint, PickResult, Today, ENERGIES, MAX_REROLLS, SETTINGS},
    weather, AppState,
};

const KM_CHOICES: [(&str, &str); 5] =
    [("15", "15 km"), ("30", "30 km"), ("45", "45 km"), ("60", "60 km"), ("100", "100 km")];

/// One slip in the jar, as the page's script sees it.
#[derive(Serialize)]
struct Slip<'a> {
    id: i64,
    title: String,
    setting: &'a str,
    /// Sinks in the jar when rain is likely.
    outdoor: bool,
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
    /// Who's holding this device ("Rohit"), empty if not chosen yet.
    me_name: String,
    /// Whose turn it is to draw ("Neha"), empty when it's anyone's.
    turn_name: String,
    /// It's this device's person's turn: they get a bonus peek.
    my_turn: bool,
    /// Tonight's rain chance (%) when rain is likely.
    rain: Option<u8>,
    /// A kept date from about a year ago this week.
    memory: Option<Memory>,
    budget: BudgetView,
    /// A gentle line: "Tomorrow, 7:00 pm: Saltt + Long drive", or "It's been 3 weeks…". Empty = none.
    nudge: String,
    /// The nudge is about a date coming up (vs. a gap since the last one).
    nudge_soon: bool,
    /// "Meet in the middle": tonight's private check-ins, from this viewer's side.
    middle: super::middle::MiddleView,
}

/// Weeks without a date before the jar says so.
const NUDGE_AFTER_DAYS: i64 = 21;

/// The gentle nudge for the jar page. A surprise sealed for this viewer shows only who it's from.
async fn nudge(s: &AppState, who: Option<usize>) -> Result<(String, bool), AppError> {
    let (today, tomorrow, _, _) = db::local_today(&s.db).await?;
    let upcoming = db::upcoming(&s.db).await?;
    if let Some(first) = upcoming.first() {
        let together: Vec<&db::HistoryRow> = upcoming.iter().filter(|r| r.planned_at == first.planned_at).collect();
        let local = first.planned_local.clone().unwrap_or_default();
        let hour: u32 = local.get(11..13).and_then(|h| h.parse().ok()).unwrap_or(0);
        let day = match local.get(..10) {
            Some(d) if d == today && hour >= 17 => "Tonight",
            Some(d) if d == today => "Today",
            Some(d) if d == tomorrow => "Tomorrow",
            _ => "Soon",
        };
        let time = super::planned_label(&local).split(", ").nth(1).unwrap_or("").to_string();
        let what: Vec<String> = together
            .iter()
            .map(|r| {
                if sealed_for(r, who) {
                    let from = r.drawn_by.and_then(|d| s.cfg.people.get(d as usize)).map_or("one of you", |p| p.0.as_str());
                    format!("a surprise from {from}")
                } else {
                    s.cfg.personalize(&r.title)
                }
            })
            .collect::<Vec<_>>();
        let mut what = what;
        what.dedup();
        return Ok((format!("{day}, {time}: {}", what.join(" + ")), true));
    }
    if !db::anything_planned(&s.db).await? {
        if let Some(days) = db::days_since_last_date(&s.db).await?.filter(|d| *d >= NUDGE_AFTER_DAYS) {
            let weeks = days / 7;
            return Ok((format!("It's been {weeks} weeks since your last date."), false));
        }
    }
    Ok((String::new(), false))
}

pub struct Memory {
    pub title: String,
    /// "14 Oct 2025"
    pub when: String,
}

/// Whose turn it is to draw: the other one of us from whoever kept the last date.
pub async fn turn(s: &AppState) -> Result<Option<usize>, AppError> {
    Ok(crate::db::last_drawer(&s.db).await?.and_then(|w| usize::try_from(1 - w).ok()))
}

/// Is a kept date a surprise this viewer shouldn't see yet? Only whoever planned it sees it
/// until it's revealed; someone who hasn't said who they are sees nothing (fail closed).
pub fn sealed_for(row: &db::HistoryRow, me: Option<usize>) -> bool {
    row.surprise && !row.revealed && (me.is_none() || row.drawn_by != me.map(|m| m as i64))
}

/// Tonight's rain chance, only when it's likely.
async fn rain(s: &AppState) -> Option<u8> {
    s.weather.tonight().await.filter(|p| *p >= weather::LIKELY_PERCENT)
}

pub async fn page(State(s): State<AppState>, jar: SignedCookieJar) -> HtmlResult {
    let who = auth::who(&jar);
    let me = Me::new(&s.cfg, who);
    let turn = turn(&s).await?;
    let memory = db::year_ago(&s.db)
        .await?
        .into_iter()
        .find(|r| !sealed_for(r, who))
        .map(|r| Memory { title: s.cfg.personalize(&r.title), when: format!("{} {}", r.day_label(), r.day.get(..4).unwrap_or("")) });
    let nudge = nudge(&s, who).await?;
    let middle = super::middle::view(&s, who, &me.partner).await?;
    let jar = jar_data(&s).await?;
    let acts = &jar.acts;
    let mut tags: Vec<String> = acts.iter().flat_map(|a| a.tag_list()).collect();
    tags.sort();
    tags.dedup();
    let slips: Vec<Slip> =
        acts.iter()
            .map(|a| Slip {
                id: a.id,
                title: s.cfg.personalize(&a.title),
                setting: &a.setting,
                outdoor: a.tag_list().iter().any(|t| t == "outdoors"),
            })
            .collect();
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
        me_name: me.name,
        turn_name: turn.map(|t| s.cfg.people[t].0.clone()).unwrap_or_default(),
        my_turn: turn.is_some() && turn == who,
        rain: rain(&s).await,
        memory,
        budget: budget_view(&s).await?,
        nudge: nudge.0,
        nudge_soon: nudge.1,
        middle,
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
    /// "1": whoever's holding the phone passes on the current slip for good ("Not for me").
    veto: String,
    /// "1": draw from tonight's check-ins ("Meet in the middle") instead of these filters.
    middle: String,
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
            rain_likely: false,
            today: None,
        }
    }

    fn is_middle(&self) -> bool {
        self.middle == "1"
    }

    /// These filters replaced by where tonight's two check-ins meet.
    fn met(&self, m: &Middle) -> PickForm {
        PickForm {
            budget: m.budget.map(|b| b.to_string()).unwrap_or_default(),
            max_duration: m.max_duration.map(|d| d.key().to_string()).unwrap_or_default(),
            settings: if m.apart {
                vec!["apart".into()]
            } else if m.stay_in {
                vec!["home".into()]
            } else {
                // Together tonight: anything but the long-distance ones.
                SETTINGS.iter().map(|(k, _)| k.to_string()).filter(|k| k != "apart").collect()
            },
            tag: String::new(),
            energy: String::new(),
            max_km: m.max_km.map(|k| k.to_string()).unwrap_or_default(),
            from: String::new(),
            ..self.clone()
        }
    }

    fn from(&self) -> Option<usize> {
        self.from.parse().ok().filter(|i| *i < 2)
    }

    fn excluded(&self) -> Vec<i64> {
        self.excluded.split(',').filter_map(|s| s.trim().parse().ok()).collect()
    }

    /// The filter fields only, as hidden inputs. Drawing from the middle round-trips just
    /// that flag: the server works the filters out again each time, so neither of us can
    /// read the other's answers out of the page.
    fn filter_fields(&self) -> Vec<(String, String)> {
        if self.is_middle() {
            return vec![("middle".into(), "1".into())];
        }
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

/// What can be drawn right now: slips in the jar that neither of us passed on, what we've
/// learned about real costs, and today's date (for closed days and seasons). Every place that
/// suggests or counts slips uses this, so they all agree.
pub struct JarData {
    pub acts: Vec<db::Activity>,
    pub learned: Learned,
    pub today: Today,
}

pub async fn jar_data(s: &AppState) -> Result<JarData, AppError> {
    let vetoed: Vec<i64> = db::vetoes(&s.db).await?.into_iter().map(|(a, _)| a).collect();
    let acts = db::list(&s.db, false).await?.into_iter().filter(|a| !vetoed.contains(&a.id)).collect();
    let (_, _, weekday, month) = db::local_today(&s.db).await?;
    Ok(JarData { acts, learned: learned(s).await?, today: Today { weekday, month } })
}

impl JarData {
    fn candidates(&self, s: &AppState, f: &PickForm) -> Vec<Candidate> {
        self.acts
            .iter()
            .map(|a| {
                let mut c = a.candidate(distance_from(&s.cfg, a, f.from()), self.learned.cost);
                c.title = s.cfg.personalize(&c.title);
                c
            })
            .collect()
    }

    fn filters(&self, f: &PickForm, rain_likely: bool) -> Filters {
        Filters { rain_likely, today: Some(self.today), ..f.filters() }
    }
}

#[derive(Serialize)]
pub struct MatchCount {
    count: usize,
    total: usize,
    ids: Vec<i64>,
}

/// Which slips fit the current filters — lets the jar react live as filters change.
pub async fn matching(State(s): State<AppState>, Form(f): Form<PickForm>) -> Result<Json<MatchCount>, AppError> {
    let jar = jar_data(&s).await?;
    let cands = jar.candidates(&s, &f);
    let filters = jar.filters(&f, rain(&s).await.is_some());
    let ids: Vec<i64> = cands.iter().filter(|c| picker::matches(c, &filters)).map(|c| c.id).collect();
    Ok(Json(MatchCount { count: ids.len(), total: cands.len(), ids }))
}

/// A slip that pairs with the drawn one, offered as "make it a pair".
pub struct PairOffer {
    pub id: i64,
    pub title: String,
    pub cost: String,
    pub duration: String,
    /// Combined cost of both, e.g. "₹4,300".
    pub together: String,
    /// e.g. "₹2,300 over tonight's budget", or empty.
    pub over: String,
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
    /// The drawn slip was generated for us, so putting it back is free.
    free_putback: bool,
    pairs: Vec<PairOffer>,
    /// On an accepted pair: the second slip.
    paired: Option<ActivityView>,
    hidden: Vec<(String, String)>,
    /// This draw has a bonus peek because it's this person's turn.
    bonus_peek: bool,
    /// We know who's holding the phone, so the slip can be kept as a surprise for the other.
    can_surprise: bool,
    /// We know who's holding the phone, so they can pass on this slip ("Not for me").
    can_veto: bool,
    /// The other one of us ("Neha"), or "your partner".
    partner: String,
    /// Accepted as a surprise (done card).
    surprise: bool,
    /// History row ids of the accepted date (two for a pair), comma separated, for "when is it?".
    history_ids: String,
    /// WhatsApp share: the message, and a ready wa.me link.
    share_text: String,
    wa_url: String,
    /// Drawn from tonight's check-ins.
    middle: bool,
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
            free_putback: false,
            middle: false,
            pairs: vec![],
            paired: None,
            hidden: vec![],
            bonus_peek: false,
            can_surprise: false,
            can_veto: false,
            partner: String::new(),
            surprise: false,
            history_ids: String::new(),
            share_text: String::new(),
            wa_url: String::new(),
        }
    }
}

/// Who's drawing and how many peeks they get. Worked out on the server every time.
struct Drawer {
    me: Me,
    max: u32,
}

async fn drawer(s: &AppState, jar: &SignedCookieJar) -> Result<Drawer, AppError> {
    let who = auth::who(jar);
    let turn = turn(s).await?;
    Ok(Drawer { me: Me::new(&s.cfg, who), max: picker::max_peeks(who.is_some() && turn == who) })
}

pub async fn pick(State(s): State<AppState>, jar: SignedCookieJar, Form(f): Form<PickForm>) -> HtmlResult {
    let d = drawer(&s, &jar).await?;
    let middle = if f.is_middle() { super::middle::tonight(&s.db).await? } else { None };
    if f.is_middle() && middle.is_none() {
        return render(CardTpl {
            message: "Once you've both checked in for tonight, the jar draws from the middle.".into(),
            ..CardTpl::blank()
        });
    }
    let f = match &middle {
        Some(m) => f.met(m),
        None => f,
    };
    let mut excluded = f.excluded();
    let mut used: u32 = f.used.parse().unwrap_or(0);

    // "Not for me": out of the draw for both of us, and it doesn't cost a peek.
    if f.veto == "1" {
        if let (Some(me), Ok(cur)) = (d.me.who, f.current.parse::<i64>()) {
            db::veto(&s.db, cur, me).await?;
            excluded.push(cur);
        }
    } else if f.reroll == "1" {
        if let Ok(cur) = f.current.parse::<i64>() {
            let current = db::get(&s.db, cur).await?;
            match picker::put_back(used, current.as_ref().is_some_and(|a| a.generated), d.max) {
                Some(next) => {
                    db::record(&s.db, cur, false).await?;
                    excluded.push(cur);
                    used = next;
                }
                // Out of peeks: the card already hides the button, so just show it again.
                None => {
                    if let Some(a) = current {
                        let learned = learned(&s).await?;
                        return card(&s, &d, &f, Some(&a), &excluded, used, &learned).await;
                    }
                }
            }
        }
    }

    let jar = jar_data(&s).await?;
    let mut cands = jar.candidates(&s, &f);
    if let Some(m) = &middle {
        cands.retain(|c| m.allows(&c.energy, &c.tags));
    }
    let filters = jar.filters(&f, rain(&s).await.is_some());
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let result = picker::pick(&cands, &filters, &excluded, now, &mut rand::thread_rng());

    match result {
        PickResult::Pick(id) => {
            card(&s, &d, &f, jar.acts.iter().find(|a| a.id == id), &excluded, used, &jar.learned).await
        }
        // Saying which filter to loosen would give away one of our answers.
        PickResult::NoMatch(h) if middle.is_some() => render(CardTpl {
            message: match h {
                Hint::Exhausted => "You've peeked at every slip that fits you both. One of you could change your check-in.".into(),
                _ => "Nothing in the jar fits you both tonight. One of you could change your check-in, or draw on your own.".into(),
            },
            ..CardTpl::blank()
        }),
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

async fn card(
    s: &AppState,
    d: &Drawer,
    f: &PickForm,
    a: Option<&db::Activity>,
    excluded: &[i64],
    used: u32,
    learned: &Learned,
) -> HtmlResult {
    let pairs = match a {
        Some(a) => pair_offers(s, f, a, learned).await?,
        None => vec![],
    };
    let view = a.map(|a| ActivityView::new(&s.cfg, a, learned));
    let share = view.as_ref().map(share_text).unwrap_or_default();
    render(CardTpl {
        hidden: f.hidden(excluded, used, a.map_or(0, |a| a.id)),
        rerolls_left: picker::rerolls_left(used, d.max),
        max_rerolls: d.max,
        bonus_peek: d.max > MAX_REROLLS,
        free_putback: a.is_some_and(|a| a.generated),
        can_surprise: d.me.who.is_some(),
        can_veto: d.me.who.is_some(),
        partner: d.me.partner.clone(),
        wa_url: format!("https://wa.me/?text={}", url_encode(&share)),
        middle: f.is_middle(),
        share_text: share,
        activity: view,
        pairs,
        ..CardTpl::blank()
    })
}

/// Up to two partners for the drawn slip, with the combined cost against tonight's budget.
async fn pair_offers(s: &AppState, f: &PickForm, a: &db::Activity, learned: &Learned) -> Result<Vec<PairOffer>, AppError> {
    let budget = f.filters().budget;
    let vetoed: Vec<i64> = db::vetoes(&s.db).await?.into_iter().map(|(a, _)| a).collect();
    Ok(db::partners(&s.db, a.id)
        .await?
        .iter()
        .filter(|p| !vetoed.contains(&p.id))
        .take(2)
        .map(|p| {
            // Same costs the jar uses everywhere else.
            let (mine, theirs) = (a.effective_cost(learned.cost), p.effective_cost(learned.cost));
            let together = mine + theirs;
            PairOffer {
                id: p.id,
                title: s.cfg.personalize(&p.title),
                cost: s.cfg.money(theirs),
                duration: p.duration().label().into(),
                together: s.cfg.money(together),
                over: if budget != i64::MAX && together > budget {
                    format!("{} over tonight's budget", s.cfg.money(together - budget))
                } else {
                    String::new()
                },
            }
        })
        .collect())
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
        Hint::ClosedToday { title, weekday } => format!(
            "{title} would fit, but it's closed on {}s.",
            picker::WEEKDAYS.get(weekday as usize).map_or("this day", |d| d.1)
        ),
        Hint::OutOfSeason { title, from, to } => {
            format!("{title} would fit, but it's only on {}.", picker::season_label(from, to))
        }
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
        // Nothing to loosen: we can't change the day or the season.
        Hint::ClosedToday { .. } | Hint::OutOfSeason { .. } => return (String::new(), None),
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

#[derive(Deserialize)]
pub struct AcceptQuery {
    /// Accept this partner slip too, as one date.
    with: Option<i64>,
    /// Keep it as a surprise for the other one of us ("1").
    #[serde(default)]
    surprise: Option<String>,
}

pub async fn accept(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    Path(id): Path<i64>,
    Query(q): Query<AcceptQuery>,
) -> HtmlResult {
    let who = auth::who(&jar);
    // A surprise needs to know who it's from; otherwise it would be sealed for both of us.
    let surprise = who.is_some() && q.surprise.as_deref() == Some("1");
    let learned = learned(&s).await?;
    let mut ids = vec![db::accept(&s.db, id, who, surprise).await?];
    let mut paired = None;
    if let Some(with) = q.with {
        // Only a real partner of this slip can ride along.
        if let Some(p) = db::partners(&s.db, id).await?.into_iter().find(|p| p.id == with) {
            ids.push(db::accept(&s.db, p.id, who, surprise).await?);
            paired = Some(ActivityView::new(&s.cfg, &p, &learned));
        }
    }
    let a = db::get(&s.db, id).await?;
    let view = a.as_ref().map(|a| ActivityView::new(&s.cfg, a, &learned));
    let share = view.as_ref().map(share_text).unwrap_or_default();
    render(CardTpl {
        done: true,
        stamp_date: db::today(&s.db).await?,
        paired,
        surprise,
        partner: Me::new(&s.cfg, who).partner,
        history_ids: ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(","),
        wa_url: format!("https://wa.me/?text={}", url_encode(&share)),
        share_text: share,
        activity: view,
        ..CardTpl::blank()
    })
}
