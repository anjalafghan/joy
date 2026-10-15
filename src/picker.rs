//! Pure pick logic. No IO: everything the picker needs is passed in, so it is
//! fully deterministic under a seeded RNG and easy to test.

use rand::Rng;

pub const MAX_REROLLS: u32 = 3;
const DAY: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Duration {
    Short,
    HalfDay,
    FullDay,
    Weekend,
}

impl Duration {
    pub const ALL: [Duration; 4] = [Self::Short, Self::HalfDay, Self::FullDay, Self::Weekend];

    pub fn key(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::HalfDay => "half_day",
            Self::FullDay => "full_day",
            Self::Weekend => "weekend",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Short => "1–2 hours",
            Self::HalfDay => "Half a day",
            Self::FullDay => "A full day",
            Self::Weekend => "A weekend",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.key() == s)
    }
}

pub const SETTINGS: [(&str, &str); 4] = [
    ("home", "At home"),
    ("local", "Around town"),
    ("day_trip", "Day trip"),
    ("travel", "Travel"),
];

pub const ENERGIES: [(&str, &str); 3] = [("low", "Low"), ("med", "Medium"), ("high", "High")];

pub fn setting_label(key: &str) -> &str {
    SETTINGS.iter().find(|(k, _)| *k == key).map(|(_, l)| *l).unwrap_or(key)
}

/// The slice of an activity the picker cares about.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub id: i64,
    pub title: String,
    pub est_cost: i64,
    pub setting: String,
    pub duration: Duration,
    pub tags: Vec<String>,
    pub energy: String,
    pub last_done_at: Option<i64>,
    pub last_skipped_at: Option<i64>,
    /// Approx road km for whichever of us is farther. None = at home / unknown.
    pub distance_km: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Filters {
    pub budget: i64,
    pub max_duration: Option<Duration>,
    /// Empty means "anywhere".
    pub settings: Vec<String>,
    pub tag: Option<String>,
    pub energy: Option<String>,
    pub max_km: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hint {
    EmptyWishlist,
    /// Everything that matched was already rerolled away.
    Exhausted,
    OverBudget { title: String, cost: i64 },
    NeedsMoreTime { title: String, duration: Duration },
    TooFar { title: String, km: i64 },
    OtherSetting { title: String, setting: String },
    NoMoodMatch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickResult {
    Pick(i64),
    NoMatch(Hint),
}

#[derive(Clone, Copy)]
struct Checks {
    budget: bool,
    duration: bool,
    distance: bool,
    setting: bool,
    mood: bool,
}

fn checks(c: &Candidate, f: &Filters) -> Checks {
    Checks {
        budget: c.est_cost <= f.budget,
        duration: f.max_duration.is_none_or(|d| c.duration <= d),
        distance: f
            .max_km
            .is_none_or(|m| c.distance_km.is_none_or(|d| d <= m)),
        setting: f.settings.is_empty() || f.settings.iter().any(|s| *s == c.setting),
        mood: f.tag.as_ref().is_none_or(|t| c.tags.iter().any(|ct| ct == t))
            && f.energy.as_ref().is_none_or(|e| *e == c.energy),
    }
}

/// Higher weight = more likely. Never-done ideas get a boost, older ones
/// climb back up over ~2 months, recently skipped ones are damped.
pub fn weight(c: &Candidate, now: i64) -> f64 {
    let mut w = match c.last_done_at {
        None => 3.0,
        Some(t) => 1.0 + ((now - t).max(0) as f64 / (30 * DAY) as f64).min(2.0),
    };
    if c.last_skipped_at.is_some_and(|t| now - t < 3 * DAY) {
        w *= 0.5;
    }
    w
}

/// Does this candidate pass every hard filter? (Exclusions aside.)
pub fn matches(c: &Candidate, f: &Filters) -> bool {
    let k = checks(c, f);
    k.budget && k.duration && k.distance && k.setting && k.mood
}

pub fn pick<R: Rng>(
    all: &[Candidate],
    f: &Filters,
    excluded: &[i64],
    now: i64,
    rng: &mut R,
) -> PickResult {
    if all.is_empty() {
        return PickResult::NoMatch(Hint::EmptyWishlist);
    }

    let matching: Vec<&Candidate> = all.iter().filter(|c| matches(c, f)).collect();
    let pool: Vec<&Candidate> = matching
        .iter()
        .copied()
        .filter(|c| !excluded.contains(&c.id))
        .collect();

    if pool.is_empty() {
        return PickResult::NoMatch(if matching.is_empty() {
            hint(all, f)
        } else {
            Hint::Exhausted
        });
    }

    let weights: Vec<f64> = pool.iter().map(|c| weight(c, now)).collect();
    let mut roll = rng.gen_range(0.0..weights.iter().sum::<f64>());
    for (c, w) in pool.iter().zip(&weights) {
        if roll < *w {
            return PickResult::Pick(c.id);
        }
        roll -= w;
    }
    PickResult::Pick(pool[pool.len() - 1].id)
}

/// Explain which single constraint to relax, preferring the cheapest change.
fn hint(all: &[Candidate], f: &Filters) -> Hint {
    // Candidates that fail exactly the one check `pred` singles out.
    let only_fails = |pred: fn(Checks) -> bool| -> Vec<&Candidate> {
        all.iter().filter(|c| pred(checks(c, f))).collect()
    };

    let mut over_budget =
        only_fails(|k| !k.budget && k.duration && k.distance && k.setting && k.mood);
    over_budget.sort_by_key(|c| c.est_cost);
    if let Some(c) = over_budget.first() {
        return Hint::OverBudget { title: c.title.clone(), cost: c.est_cost };
    }

    let mut longer =
        only_fails(|k| k.budget && !k.duration && k.distance && k.setting && k.mood);
    longer.sort_by_key(|c| c.duration);
    if let Some(c) = longer.first() {
        return Hint::NeedsMoreTime { title: c.title.clone(), duration: c.duration };
    }

    let mut farther =
        only_fails(|k| k.budget && k.duration && !k.distance && k.setting && k.mood);
    farther.sort_by(|a, b| a.distance_km.partial_cmp(&b.distance_km).unwrap());
    if let Some(c) = farther.first() {
        return Hint::TooFar { title: c.title.clone(), km: c.distance_km.unwrap_or(0.0).round() as i64 };
    }

    if let Some(c) = only_fails(|k| k.budget && k.duration && k.distance && !k.setting && k.mood)
        .first()
    {
        return Hint::OtherSetting { title: c.title.clone(), setting: c.setting.clone() };
    }

    Hint::NoMoodMatch
}

pub fn rerolls_left(used: u32) -> u32 {
    MAX_REROLLS.saturating_sub(used)
}

/// Peeks used after putting the current slip back, or `None` when it has to stay.
/// Slips the app generated go back for free, so you can browse them without limit.
pub fn put_back(used: u32, generated: bool) -> Option<u32> {
    match (generated, used < MAX_REROLLS) {
        (true, _) => Some(used),
        (false, true) => Some(used + 1),
        (false, false) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    const NOW: i64 = 1_800_000_000;

    fn cand(id: i64, cost: i64, setting: &str, duration: Duration) -> Candidate {
        Candidate {
            id,
            title: format!("idea {id}"),
            est_cost: cost,
            setting: setting.into(),
            duration,
            tags: vec!["chill".into()],
            energy: "med".into(),
            last_done_at: None,
            last_skipped_at: None,
            distance_km: None,
        }
    }

    fn filters(budget: i64) -> Filters {
        Filters { budget, ..Default::default() }
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    fn picks(all: &[Candidate], f: &Filters, excluded: &[i64], n: usize) -> Vec<PickResult> {
        let mut r = rng();
        (0..n).map(|_| pick(all, f, excluded, NOW, &mut r)).collect()
    }

    #[test]
    fn budget_is_a_hard_filter() {
        let all = [cand(1, 500, "home", Duration::Short), cand(2, 5000, "home", Duration::Short)];
        assert!(picks(&all, &filters(1000), &[], 200).iter().all(|p| *p == PickResult::Pick(1)));
    }

    #[test]
    fn duration_is_a_hard_filter_and_ordered() {
        let all = [cand(1, 0, "home", Duration::HalfDay), cand(2, 0, "home", Duration::Weekend)];
        let f = Filters { max_duration: Some(Duration::FullDay), ..filters(0) };
        assert!(picks(&all, &f, &[], 200).iter().all(|p| *p == PickResult::Pick(1)));
    }

    #[test]
    fn setting_is_a_hard_filter_and_empty_means_any() {
        let all = [cand(1, 0, "home", Duration::Short), cand(2, 0, "travel", Duration::Short)];
        let f = Filters { settings: vec!["travel".into()], ..filters(0) };
        assert!(picks(&all, &f, &[], 200).iter().all(|p| *p == PickResult::Pick(2)));
        let any = picks(&all, &filters(0), &[], 200);
        assert!(any.contains(&PickResult::Pick(1)) && any.contains(&PickResult::Pick(2)));
    }

    #[test]
    fn tag_and_energy_are_hard_filters() {
        let mut a = cand(1, 0, "home", Duration::Short);
        a.tags = vec!["foodie".into()];
        let mut b = cand(2, 0, "home", Duration::Short);
        b.energy = "high".into();
        let all = [a, b];
        let f = Filters { tag: Some("foodie".into()), ..filters(0) };
        assert!(picks(&all, &f, &[], 100).iter().all(|p| *p == PickResult::Pick(1)));
        let f = Filters { energy: Some("high".into()), ..filters(0) };
        assert!(picks(&all, &f, &[], 100).iter().all(|p| *p == PickResult::Pick(2)));
    }

    #[test]
    fn distance_is_a_hard_filter_and_home_always_passes() {
        let mut near = cand(1, 0, "local", Duration::Short);
        near.distance_km = Some(8.0);
        let mut far = cand(2, 0, "local", Duration::Short);
        far.distance_km = Some(60.0);
        let home = cand(3, 0, "home", Duration::Short);
        let f = Filters { max_km: Some(20.0), ..filters(0) };
        let r = picks(&[near, far.clone(), home], &f, &[], 300);
        assert!(r.iter().all(|p| *p != PickResult::Pick(2)));
        assert!(r.contains(&PickResult::Pick(1)) && r.contains(&PickResult::Pick(3)));
        assert_eq!(
            picks(&[far], &f, &[], 1)[0],
            PickResult::NoMatch(Hint::TooFar { title: "idea 2".into(), km: 60 })
        );
    }

    #[test]
    fn matches_agrees_with_pick() {
        let all = [cand(1, 500, "home", Duration::Short), cand(2, 5000, "local", Duration::Weekend)];
        let f = Filters { max_duration: Some(Duration::FullDay), ..filters(1000) };
        let ids: Vec<i64> = all.iter().filter(|c| matches(c, &f)).map(|c| c.id).collect();
        assert_eq!(ids, vec![1]);
        assert!(picks(&all, &f, &[], 50).iter().all(|p| *p == PickResult::Pick(1)));
    }

    #[test]
    fn excluded_ids_are_never_returned() {
        let all: Vec<_> = (1..=5).map(|i| cand(i, 0, "home", Duration::Short)).collect();
        assert!(picks(&all, &filters(0), &[2, 4], 500)
            .iter()
            .all(|p| !matches!(p, PickResult::Pick(2) | PickResult::Pick(4))));
    }

    #[test]
    fn exhausted_when_everything_matching_was_rerolled() {
        let all = [cand(1, 0, "home", Duration::Short)];
        assert_eq!(picks(&all, &filters(0), &[1], 1)[0], PickResult::NoMatch(Hint::Exhausted));
    }

    #[test]
    fn hints_name_the_constraint_to_relax() {
        assert_eq!(picks(&[], &filters(0), &[], 1)[0], PickResult::NoMatch(Hint::EmptyWishlist));

        let all = [cand(1, 3000, "home", Duration::Short), cand(2, 1200, "home", Duration::Short)];
        assert_eq!(
            picks(&all, &filters(500), &[], 1)[0],
            PickResult::NoMatch(Hint::OverBudget { title: "idea 2".into(), cost: 1200 })
        );

        let all = [cand(1, 0, "home", Duration::Weekend)];
        let f = Filters { max_duration: Some(Duration::Short), ..filters(0) };
        assert_eq!(
            picks(&all, &f, &[], 1)[0],
            PickResult::NoMatch(Hint::NeedsMoreTime { title: "idea 1".into(), duration: Duration::Weekend })
        );

        let f = Filters { settings: vec!["travel".into()], ..filters(0) };
        assert_eq!(
            picks(&all, &f, &[], 1)[0],
            PickResult::NoMatch(Hint::OtherSetting { title: "idea 1".into(), setting: "home".into() })
        );

        let f = Filters { tag: Some("nope".into()), ..filters(0) };
        assert_eq!(picks(&all, &f, &[], 1)[0], PickResult::NoMatch(Hint::NoMoodMatch));
    }

    #[test]
    fn never_done_beats_recently_done() {
        let fresh = cand(1, 0, "home", Duration::Short);
        let mut stale = cand(2, 0, "home", Duration::Short);
        stale.last_done_at = Some(NOW - DAY);
        let results = picks(&[fresh, stale], &filters(0), &[], 4000);
        let fresh_wins = results.iter().filter(|p| **p == PickResult::Pick(1)).count();
        // weights 3.0 vs ~1.03 -> ~74%
        assert!((2700..3200).contains(&fresh_wins), "fresh won {fresh_wins}/4000");
    }

    #[test]
    fn weight_recovers_with_time_and_is_damped_by_skips() {
        let mut c = cand(1, 0, "home", Duration::Short);
        c.last_done_at = Some(NOW - 10 * DAY);
        let recent = weight(&c, NOW);
        c.last_done_at = Some(NOW - 100 * DAY);
        let old = weight(&c, NOW);
        assert!(old > recent);
        assert_eq!(old, 3.0, "capped");
        c.last_skipped_at = Some(NOW - DAY);
        assert_eq!(weight(&c, NOW), 1.5);
    }

    #[test]
    fn generated_slips_go_back_for_free() {
        assert_eq!(put_back(0, false), Some(1));
        assert_eq!(put_back(MAX_REROLLS - 1, false), Some(MAX_REROLLS));
        assert_eq!(put_back(MAX_REROLLS, false), None);
        assert_eq!(put_back(0, true), Some(0));
        assert_eq!(put_back(MAX_REROLLS, true), Some(MAX_REROLLS), "free even when out of peeks");
    }

    #[test]
    fn reroll_cap() {
        assert_eq!(rerolls_left(0), MAX_REROLLS);
        assert_eq!(rerolls_left(MAX_REROLLS), 0);
        assert_eq!(rerolls_left(MAX_REROLLS + 5), 0);
    }
}
