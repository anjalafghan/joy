use serde::Deserialize;
use sqlx::{FromRow, SqlitePool};

use crate::picker::{Candidate, Duration};

#[derive(Debug, Clone, FromRow)]
pub struct Activity {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub est_cost: i64,
    pub setting: String,
    pub area: String,
    pub venue: String,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub duration: String,
    pub tags: String,
    pub energy: String,
    pub added_by: String,
    pub times_done: i64,
    pub last_done_at: Option<i64>,
    pub last_skipped_at: Option<i64>,
    /// Suggested by the app rather than written by us ("generated for you").
    pub generated: bool,
    /// "mon,wed": days it's closed.
    pub closed_days: String,
    pub season_from: Option<i64>,
    pub season_to: Option<i64>,
    /// The estimate came from a real cost, so the learned factor leaves it alone.
    pub est_from_actual: bool,
}

impl Activity {
    pub fn latlng(&self) -> Option<(f64, f64)> {
        Some((self.lat?, self.lng?))
    }

    pub fn tag_list(&self) -> Vec<String> {
        self.tags.split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect()
    }

    pub fn duration(&self) -> Duration {
        Duration::parse(&self.duration).unwrap_or(Duration::Short)
    }

    pub fn season(&self) -> Option<(u8, u8)> {
        let ok = |m: i64| u8::try_from(m).ok().filter(|m| (1..=12).contains(m));
        Some((ok(self.season_from?)?, ok(self.season_to?)?))
    }

    /// The estimate as the jar should use it: adjusted by what our dates really cost,
    /// unless it was already set from a real cost.
    pub fn effective_cost(&self, factor: Option<f64>) -> i64 {
        match factor {
            Some(f) if !self.est_from_actual && self.est_cost > 0 => ((self.est_cost as f64 * f) / 50.0).round() as i64 * 50,
            _ => self.est_cost,
        }
    }

    pub fn candidate(&self, distance_km: Option<f64>, factor: Option<f64>) -> Candidate {
        Candidate {
            id: self.id,
            title: self.title.clone(),
            est_cost: self.effective_cost(factor),
            setting: self.setting.clone(),
            duration: self.duration(),
            tags: self.tag_list(),
            energy: self.energy.clone(),
            last_done_at: self.last_done_at,
            last_skipped_at: self.last_skipped_at,
            distance_km,
            closed_days: crate::picker::parse_closed(&self.closed_days),
            season: self.season(),
        }
    }
}

/// Fields a person edits on the add/edit form.
#[derive(Debug, Deserialize)]
pub struct ActivityInput {
    pub title: String,
    #[serde(default)]
    pub notes: String,
    #[serde(deserialize_with = "blank_as_zero")]
    pub est_cost: i64,
    pub setting: String,
    pub duration: String,
    #[serde(default)]
    pub venue: String,
    #[serde(default)]
    pub area: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub tags: String,
    pub energy: String,
    #[serde(default)]
    pub added_by: String,
    /// Ids of slips this one pairs well with.
    #[serde(default)]
    pub pairs: Vec<i64>,
    /// Checked weekday keys it's closed on ("mon", …).
    #[serde(default)]
    pub closed: Vec<String>,
    /// Season months "1"–"12", or empty for all year.
    #[serde(default)]
    pub season_from: String,
    #[serde(default)]
    pub season_to: String,
}

/// An empty cost field means free, not a bad request.
fn blank_as_zero<'de, D: serde::Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let s = String::deserialize(d)?;
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }
    s.parse().map_err(serde::de::Error::custom)
}

const COLS: &str = "id, title, notes, est_cost, setting, area, venue, lat, lng, duration, tags, energy, \
                    added_by, times_done, last_done_at, last_skipped_at, generated, \
                    closed_days, season_from, season_to, est_from_actual";

pub async fn list(db: &SqlitePool, archived: bool) -> sqlx::Result<Vec<Activity>> {
    sqlx::query_as(&format!(
        "SELECT {COLS} FROM activities WHERE archived = ? ORDER BY est_cost, title"
    ))
    .bind(archived)
    .fetch_all(db)
    .await
}

pub async fn get(db: &SqlitePool, id: i64) -> sqlx::Result<Option<Activity>> {
    sqlx::query_as(&format!("SELECT {COLS} FROM activities WHERE id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Insert or update a slip and its pairs; returns its id.
pub async fn save(db: &SqlitePool, id: Option<i64>, a: &ActivityInput, latlng: Option<(f64, f64)>) -> sqlx::Result<i64> {
    let tags = a
        .tags
        .split(',')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(",");
    let sql = match id {
        None => "INSERT INTO activities (title, notes, est_cost, setting, duration, venue, area, lat, lng, tags, energy, added_by,
                 closed_days, season_from, season_to, id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        // A hand-edited estimate is ours again, so the learned factor applies to it.
        Some(_) => "UPDATE activities SET title = ?1, notes = ?2,
                    est_from_actual = CASE WHEN est_cost = ?3 THEN est_from_actual ELSE 0 END, est_cost = ?3,
                    setting = ?4, duration = ?5, venue = ?6, area = ?7, lat = ?8, lng = ?9, tags = ?10, energy = ?11,
                    added_by = ?12, closed_days = ?13, season_from = ?14, season_to = ?15 WHERE id = ?16",
    };
    let closed = crate::picker::WEEKDAYS
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| a.closed.iter().any(|c| c == k))
        .collect::<Vec<_>>()
        .join(",");
    let month = |s: &str| s.trim().parse::<i64>().ok().filter(|m| (1..=12).contains(m));
    let season = month(&a.season_from).zip(month(&a.season_to));
    let done = sqlx::query(sql)
        .bind(a.title.split_whitespace().collect::<Vec<_>>().join(" "))
        .bind(a.notes.trim())
        .bind(a.est_cost.max(0))
        .bind(&a.setting)
        .bind(&a.duration)
        .bind(a.venue.trim())
        .bind(a.area.trim())
        .bind(latlng.map(|l| l.0))
        .bind(latlng.map(|l| l.1))
        .bind(tags)
        .bind(&a.energy)
        .bind(a.added_by.trim())
        .bind(closed)
        .bind(season.map(|s| s.0))
        .bind(season.map(|s| s.1))
        .bind(id)
        .execute(db)
        .await?;
    // From this statement's own result: a separate query could run on another pool connection.
    let id = id.unwrap_or(done.last_insert_rowid());
    set_pairs(db, id, &a.pairs).await?;
    Ok(id)
}

/// All pairs as (smaller id, larger id).
pub async fn pairs(db: &SqlitePool) -> sqlx::Result<Vec<(i64, i64)>> {
    sqlx::query_as("SELECT a_id, b_id FROM pairs").fetch_all(db).await
}

/// Slips still in the jar that pair with `id`.
pub async fn partners(db: &SqlitePool, id: i64) -> sqlx::Result<Vec<Activity>> {
    sqlx::query_as(&format!(
        "SELECT {COLS} FROM activities WHERE archived = 0 AND id IN
           (SELECT b_id FROM pairs WHERE a_id = ?1 UNION SELECT a_id FROM pairs WHERE b_id = ?1)
         ORDER BY est_cost, title"
    ))
    .bind(id)
    .fetch_all(db)
    .await
}

/// Replace everything `id` pairs with.
pub async fn set_pairs(db: &SqlitePool, id: i64, with: &[i64]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM pairs WHERE a_id = ?1 OR b_id = ?1").bind(id).execute(&mut *tx).await?;
    for &other in with.iter().filter(|&&o| o != id) {
        sqlx::query("INSERT OR IGNORE INTO pairs (a_id, b_id) VALUES (?, ?)")
            .bind(id.min(other))
            .bind(id.max(other))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

pub async fn set_archived(db: &SqlitePool, id: i64, archived: bool) -> sqlx::Result<()> {
    sqlx::query("UPDATE activities SET archived = ? WHERE id = ?").bind(archived).bind(id).execute(db).await?;
    Ok(())
}

pub async fn delete(db: &SqlitePool, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM activities WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

pub async fn record(db: &SqlitePool, id: i64, did_it: bool) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO history (activity_id, outcome) VALUES (?, ?)")
        .bind(id)
        .bind(if did_it { "did_it" } else { "skipped" })
        .execute(&mut *tx)
        .await?;
    let sql = if did_it {
        "UPDATE activities SET times_done = times_done + 1, last_done_at = unixepoch() WHERE id = ?"
    } else {
        "UPDATE activities SET last_skipped_at = unixepoch() WHERE id = ?"
    };
    sqlx::query(sql).bind(id).execute(&mut *tx).await?;
    tx.commit().await
}

/// Accept a pick and return its history row. A double-tap within a minute only counts once
/// (the second tap gets the same row back).
pub async fn accept(db: &SqlitePool, id: i64, drawn_by: Option<usize>, surprise: bool) -> sqlx::Result<i64> {
    let mut tx = db.begin().await?;
    // One statement, so two concurrent taps can't both pass the "recent?" check.
    let inserted = sqlx::query(
        "INSERT INTO history (activity_id, outcome, drawn_by, surprise, est_cost)
         SELECT ?1, 'did_it', ?2, ?3, (SELECT est_cost FROM activities WHERE id = ?1)
         WHERE NOT EXISTS (SELECT 1 FROM history WHERE activity_id = ?1 AND outcome = 'did_it'
                           AND picked_at > unixepoch() - 60)",
    )
    .bind(id)
    .bind(drawn_by.map(|w| w as i64))
    .bind(surprise)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 1 {
        sqlx::query("UPDATE activities SET times_done = times_done + 1, last_done_at = unixepoch() WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    let row: i64 = sqlx::query_scalar(
        "SELECT id FROM history WHERE activity_id = ? AND outcome = 'did_it' ORDER BY id DESC LIMIT 1",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(row)
}

/// Today's local date for the "it's a date" stamp, e.g. "14 · 10 · 2026".
pub async fn today(db: &SqlitePool) -> sqlx::Result<String> {
    sqlx::query_scalar("SELECT strftime('%d · %m · %Y', 'now', 'localtime')").fetch_one(db).await
}

#[derive(Debug, Clone, FromRow)]
pub struct HistoryRow {
    pub id: i64,
    pub activity_id: i64,
    /// Unix seconds when it was drawn/kept.
    pub picked_at: i64,
    pub title: String,
    pub notes: String,
    pub setting: String,
    pub place: String,
    pub duration: String,
    pub energy: String,
    pub tags: String,
    /// The slip's estimate when it was kept (total for both of us).
    pub est_cost: i64,
    /// What it really cost, when someone filled it in.
    pub actual_cost: Option<i64>,
    /// Real one-way cab fares per person, when filled in.
    pub cab_fare_0: Option<i64>,
    pub cab_fare_1: Option<i64>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    /// "YYYY-MM-DD"
    pub day: String,
    pub outcome: String,
    pub rating: Option<i64>,
    pub note: String,
    pub drawn_by: Option<i64>,
    pub planned_at: Option<i64>,
    /// Local time of `planned_at` as "YYYY-MM-DD HH:MM W" (W = weekday, 0 = Sunday).
    pub planned_local: Option<String>,
    pub surprise: bool,
    pub revealed: bool,
    pub has_photo: bool,
}

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

impl HistoryRow {
    /// "14 Oct"
    pub fn day_label(&self) -> String {
        let mut parts = self.day.split('-').skip(1);
        let (m, d) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let month = MONTHS.get(m.parse::<usize>().unwrap_or(1).saturating_sub(1)).copied().unwrap_or("");
        format!("{} {}", d.trim_start_matches('0'), &month[..month.len().min(3)])
    }

    /// "October 2026"
    pub fn month_label(&self) -> String {
        let mut parts = self.day.split('-');
        let (y, m) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let month = MONTHS.get(m.parse::<usize>().unwrap_or(1).saturating_sub(1)).copied().unwrap_or("");
        format!("{month} {y}")
    }
}

const HISTORY_COLS: &str = "h.id, h.activity_id, h.picked_at, a.title, a.notes, a.setting,
    CASE WHEN a.venue != '' THEN a.venue ELSE a.area END AS place,
    a.duration, a.energy, a.tags, COALESCE(h.est_cost, a.est_cost) AS est_cost, h.actual_cost, h.cab_fare_0, h.cab_fare_1, a.lat, a.lng,
    date(h.picked_at, 'unixepoch', 'localtime') AS day, h.outcome, h.rating, h.note,
    h.drawn_by, h.planned_at,
    strftime('%Y-%m-%d %H:%M %w', h.planned_at, 'unixepoch', 'localtime') AS planned_local,
    h.surprise, h.revealed, h.has_photo";

pub async fn history(db: &SqlitePool) -> sqlx::Result<Vec<HistoryRow>> {
    sqlx::query_as(&format!(
        "SELECT {HISTORY_COLS} FROM history h JOIN activities a ON a.id = h.activity_id
         ORDER BY h.picked_at DESC, h.id DESC LIMIT 300"
    ))
    .fetch_all(db)
    .await
}

pub async fn history_row(db: &SqlitePool, id: i64) -> sqlx::Result<Option<HistoryRow>> {
    sqlx::query_as(&format!("SELECT {HISTORY_COLS} FROM history h JOIN activities a ON a.id = h.activity_id WHERE h.id = ?"))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Kept dates from about this week, one year ago.
pub async fn year_ago(db: &SqlitePool) -> sqlx::Result<Vec<HistoryRow>> {
    sqlx::query_as(&format!(
        "SELECT {HISTORY_COLS} FROM history h JOIN activities a ON a.id = h.activity_id
         WHERE h.outcome = 'did_it'
           AND date(h.picked_at, 'unixepoch', 'localtime')
               BETWEEN date('now', 'localtime', '-1 year', '-3 days') AND date('now', 'localtime', '-1 year', '+3 days')
         ORDER BY h.picked_at"
    ))
    .fetch_all(db)
    .await
}

/// Set when these dates happen, from a local "YYYY-MM-DD HH:MM" (or clear it with None).
pub async fn set_planned(db: &SqlitePool, ids: &[i64], local: Option<&str>) -> sqlx::Result<()> {
    for id in ids {
        sqlx::query("UPDATE history SET planned_at = unixepoch(?, 'utc') WHERE id = ? AND outcome = 'did_it'")
            .bind(local)
            .bind(id)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// Remove a kept date. The slip stays in the jar; its "done" count and last-done
/// time are recalculated from what's left, so the picker's weighting stays right.
pub async fn delete_history(db: &SqlitePool, id: i64) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    let activity: Option<i64> = sqlx::query_scalar("SELECT activity_id FROM history WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM history WHERE id = ?").bind(id).execute(&mut *tx).await?;
    if let Some(a) = activity {
        sqlx::query(
            "UPDATE activities SET
               times_done = (SELECT COUNT(*) FROM history WHERE activity_id = ?1 AND outcome = 'did_it'),
               last_done_at = (SELECT MAX(picked_at) FROM history WHERE activity_id = ?1 AND outcome = 'did_it')
             WHERE id = ?1",
        )
        .bind(a)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Record what a kept date really cost (None clears it). Optionally teach the slip:
/// set its estimate to the real cost, so future draws and budget filters are closer.
pub async fn set_actual(db: &SqlitePool, id: i64, actual: Option<i64>, update_estimate: bool) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE history SET actual_cost = ? WHERE id = ? AND outcome = 'did_it'")
        .bind(actual)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if let (Some(cost), true) = (actual, update_estimate) {
        sqlx::query("UPDATE activities SET est_cost = ?, est_from_actual = 1 WHERE id = (SELECT activity_id FROM history WHERE id = ?)")
            .bind(cost)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

pub async fn reveal(db: &SqlitePool, id: i64) -> sqlx::Result<()> {
    sqlx::query("UPDATE history SET revealed = 1 WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

pub async fn set_photo(db: &SqlitePool, id: i64, has: bool) -> sqlx::Result<()> {
    sqlx::query("UPDATE history SET has_photo = ? WHERE id = ?").bind(has).bind(id).execute(db).await?;
    Ok(())
}

/// Who kept the most recent date (0 or 1), if we know.
pub async fn last_drawer(db: &SqlitePool) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "SELECT drawn_by FROM history WHERE outcome = 'did_it' AND drawn_by IS NOT NULL ORDER BY id DESC LIMIT 1",
    )
    .fetch_optional(db)
    .await
}

/// What we really spent on dates this calendar month (local time). Only dates with an
/// actual cost filled in count: a kept slip that hasn't happened yet isn't money spent.
/// A date counts in the month it happens (its planned time), else the month it was drawn.
/// Spending logged outside a slip counts in the month of its date.
pub async fn spent_this_month(db: &SqlitePool) -> sqlx::Result<i64> {
    spent_in_month(db, &month_offset(db, 0).await?).await
}

/// "YYYY-MM" of this calendar month (local time), `back` months ago.
pub async fn month_offset(db: &SqlitePool, back: i64) -> sqlx::Result<String> {
    sqlx::query_scalar("SELECT strftime('%Y-%m', 'now', 'localtime', 'start of month', '-' || ? || ' months')")
        .bind(back)
        .fetch_one(db)
        .await
}

/// Spent in the given "YYYY-MM" (see `spent_this_month`).
pub async fn spent_in_month(db: &SqlitePool, month: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT
           (SELECT COALESCE(SUM(actual_cost), 0) FROM history
             WHERE outcome = 'did_it' AND actual_cost IS NOT NULL
               AND strftime('%Y-%m', COALESCE(planned_at, picked_at), 'unixepoch', 'localtime') = ?1)
         + (SELECT COALESCE(SUM(amount), 0) FROM other_spending WHERE strftime('%Y-%m', spent_on) = ?1)",
    )
    .bind(month)
    .fetch_one(db)
    .await
}

/// What we spent in each month of `year` (index 0 = January), for the year in review: real costs of
/// dates both of us may see (no unrevealed surprises, nothing still to come) plus spending that
/// wasn't a slip.
pub async fn spent_by_month(db: &SqlitePool, year: i32) -> sqlx::Result<[i64; 12]> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT m, SUM(a) FROM (
           SELECT CAST(strftime('%m', COALESCE(planned_at, picked_at), 'unixepoch', 'localtime') AS INTEGER) AS m, actual_cost AS a
             FROM history
            WHERE outcome = 'did_it' AND actual_cost IS NOT NULL AND (surprise = 0 OR revealed = 1)
              AND COALESCE(planned_at, picked_at) <= unixepoch()
              AND CAST(strftime('%Y', COALESCE(planned_at, picked_at), 'unixepoch', 'localtime') AS INTEGER) = ?1
           UNION ALL
           SELECT CAST(strftime('%m', spent_on) AS INTEGER), amount FROM other_spending
            WHERE CAST(strftime('%Y', spent_on) AS INTEGER) = ?1
         ) GROUP BY m",
    )
    .bind(year)
    .fetch_all(db)
    .await?;
    let mut out = [0; 12];
    for (m, total) in rows {
        if let Some(slot) = usize::try_from(m - 1).ok().and_then(|i| out.get_mut(i)) {
            *slot = total;
        }
    }
    Ok(out)
}

/// What we decided at the start of a month: the budget then, and money rolled over into it.
pub async fn month_budget(db: &SqlitePool, month: &str) -> sqlx::Result<Option<(i64, i64)>> {
    sqlx::query_as("SELECT base, carry FROM month_budget WHERE month = ?").bind(month).fetch_optional(db).await
}

/// Settle a month's start: rolling over `carry` (0 = start fresh). Pressing twice changes nothing.
pub async fn decide_month(db: &SqlitePool, month: &str, base: i64, carry: i64) -> sqlx::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO month_budget (month, base, carry) VALUES (?, ?, ?)")
        .bind(month)
        .bind(base)
        .bind(carry)
        .execute(db)
        .await?;
    Ok(())
}

/// Finished months with anything spent, newest first: (month "YYYY-MM", spent).
pub async fn past_months(db: &SqlitePool) -> sqlx::Result<Vec<(String, i64)>> {
    sqlx::query_as(
        "SELECT m, SUM(a) FROM (
           SELECT strftime('%Y-%m', COALESCE(planned_at, picked_at), 'unixepoch', 'localtime') AS m, actual_cost AS a
             FROM history WHERE outcome = 'did_it' AND actual_cost IS NOT NULL
           UNION ALL
           SELECT strftime('%Y-%m', spent_on), amount FROM other_spending
         ) WHERE m < strftime('%Y-%m', 'now', 'localtime') GROUP BY m HAVING SUM(a) > 0 ORDER BY m DESC LIMIT 12",
    )
    .fetch_all(db)
    .await
}

/// Log money spent on a date that wasn't a slip. `spent_on` is "YYYY-MM-DD"; blank means today.
pub async fn add_other_spending(db: &SqlitePool, amount: i64, note: &str, spent_on: &str) -> sqlx::Result<()> {
    sqlx::query("INSERT INTO other_spending (amount, note, spent_on) VALUES (?, ?, COALESCE(NULLIF(date(?), ''), date('now', 'localtime')))")
        .bind(amount)
        .bind(note.trim())
        .bind(spent_on.trim())
        .execute(db)
        .await?;
    Ok(())
}

pub async fn delete_other_spending(db: &SqlitePool, id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM other_spending WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

/// This month's non-slip spending, newest first: (id, amount, note, spent_on).
pub async fn other_spending_this_month(db: &SqlitePool) -> sqlx::Result<Vec<(i64, i64, String, String)>> {
    sqlx::query_as(
        "SELECT id, amount, note, spent_on FROM other_spending
         WHERE strftime('%Y-%m', spent_on) = strftime('%Y-%m', 'now', 'localtime')
         ORDER BY spent_on DESC, id DESC",
    )
    .fetch_all(db)
    .await
}

/// Non-slip spending in a "YYYY-MM", oldest first: (amount, note, spent_on).
pub async fn other_spending_in_month(db: &SqlitePool, month: &str) -> sqlx::Result<Vec<(i64, String, String)>> {
    sqlx::query_as(
        "SELECT amount, note, spent_on FROM other_spending WHERE strftime('%Y-%m', spent_on) = ? ORDER BY spent_on, id",
    )
    .bind(month)
    .fetch_all(db)
    .await
}

pub async fn setting(db: &SqlitePool, key: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT value FROM settings WHERE key = ?").bind(key).fetch_optional(db).await
}

pub async fn set_setting(db: &SqlitePool, key: &str, value: Option<&str>) -> sqlx::Result<()> {
    match value {
        Some(v) => sqlx::query("INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
            .bind(key)
            .bind(v)
            .execute(db)
            .await?,
        None => sqlx::query("DELETE FROM settings WHERE key = ?").bind(key).execute(db).await?,
    };
    Ok(())
}

pub async fn rate(db: &SqlitePool, id: i64, rating: Option<i64>, note: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE history SET rating = ?, note = ? WHERE id = ?")
        .bind(rating.map(|r| r.clamp(1, 5)))
        .bind(note.trim())
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

// ── learned from real costs ───────────────────────────────────────────────

/// Fewest samples before we trust what we've learned.
pub const MIN_SAMPLES: i64 = 3;

/// How real plan costs compare to estimates: (sum actual ÷ sum estimate, samples).
pub async fn cost_ratio(db: &SqlitePool) -> sqlx::Result<(f64, i64)> {
    let (actual, est, n): (Option<i64>, Option<i64>, i64) = sqlx::query_as(
        "SELECT SUM(actual_cost), SUM(est_cost), COUNT(*) FROM history
         WHERE outcome = 'did_it' AND actual_cost IS NOT NULL AND est_cost > 0",
    )
    .fetch_one(db)
    .await?;
    Ok(match (actual, est) {
        (Some(a), Some(e)) if e > 0 => (a as f64 / e as f64, n),
        _ => (1.0, 0),
    })
}

/// Real cab fares vs the model's estimates at the time: (ratio, rides).
pub async fn cab_ratio(db: &SqlitePool) -> sqlx::Result<(f64, i64)> {
    let (fare, est, n): (Option<i64>, Option<i64>, i64) = sqlx::query_as(
        "SELECT SUM(fare), SUM(est), COUNT(*) FROM (
           SELECT cab_fare_0 AS fare, cab_est_0 AS est FROM history WHERE cab_fare_0 > 0 AND cab_est_0 > 0
           UNION ALL
           SELECT cab_fare_1, cab_est_1 FROM history WHERE cab_fare_1 > 0 AND cab_est_1 > 0)",
    )
    .fetch_one(db)
    .await?;
    Ok(match (fare, est) {
        (Some(f), Some(e)) if e > 0 => (f as f64 / e as f64, n),
        _ => (1.0, 0),
    })
}

/// Record a real one-way cab fare for one of us, next to the model's estimate for that trip.
pub async fn set_cab_fare(db: &SqlitePool, id: i64, person: usize, fare: Option<i64>, model_est: Option<i64>) -> sqlx::Result<()> {
    let sql = if person == 0 {
        "UPDATE history SET cab_fare_0 = ?, cab_est_0 = ? WHERE id = ? AND outcome = 'did_it'"
    } else {
        "UPDATE history SET cab_fare_1 = ?, cab_est_1 = ? WHERE id = ? AND outcome = 'did_it'"
    };
    sqlx::query(sql).bind(fare).bind(fare.and(model_est)).bind(id).execute(db).await?;
    Ok(())
}

// ── vetoes ────────────────────────────────────────────────────────────────

/// (activity id, who passed on it).
pub async fn vetoes(db: &SqlitePool) -> sqlx::Result<Vec<(i64, i64)>> {
    sqlx::query_as("SELECT activity_id, person FROM vetoes").fetch_all(db).await
}

pub async fn veto(db: &SqlitePool, activity: i64, person: usize) -> sqlx::Result<()> {
    sqlx::query("INSERT OR IGNORE INTO vetoes (activity_id, person) VALUES (?, ?)")
        .bind(activity)
        .bind(person as i64)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn unveto(db: &SqlitePool, activity: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM vetoes WHERE activity_id = ?").bind(activity).execute(db).await?;
    Ok(())
}

// ── gentle nudges ─────────────────────────────────────────────────────────

/// Today and tomorrow as local "YYYY-MM-DD", plus today's weekday (0 = Sunday) and month.
pub async fn local_today(db: &SqlitePool) -> sqlx::Result<(String, String, u8, u8)> {
    let (today, tomorrow, wd, m): (String, String, i64, i64) = sqlx::query_as(
        "SELECT date('now', 'localtime'), date('now', 'localtime', '+1 day'),
                CAST(strftime('%w', 'now', 'localtime') AS INTEGER), CAST(strftime('%m', 'now', 'localtime') AS INTEGER)",
    )
    .fetch_one(db)
    .await?;
    Ok((today, tomorrow, wd as u8, m as u8))
}

/// Kept dates planned from now until the end of tomorrow (local), soonest first.
pub async fn upcoming(db: &SqlitePool) -> sqlx::Result<Vec<HistoryRow>> {
    sqlx::query_as(&format!(
        "SELECT {HISTORY_COLS} FROM history h JOIN activities a ON a.id = h.activity_id
         WHERE h.outcome = 'did_it' AND h.planned_at >= unixepoch() - 3 * 3600
           AND date(h.planned_at, 'unixepoch', 'localtime') <= date('now', 'localtime', '+1 day')
         ORDER BY h.planned_at, h.id"
    ))
    .fetch_all(db)
    .await
}

/// Is anything planned for later (beyond tomorrow too)?
pub async fn anything_planned(db: &SqlitePool) -> sqlx::Result<bool> {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM history WHERE outcome = 'did_it' AND planned_at > unixepoch())")
        .fetch_one(db)
        .await
}

/// Whole days since our last date happened (its planned time if set, else when it was kept).
pub async fn days_since_last_date(db: &SqlitePool) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar(
        "SELECT CAST((unixepoch() - MAX(COALESCE(planned_at, picked_at))) / 86400 AS INTEGER) FROM history
         WHERE outcome = 'did_it' AND COALESCE(planned_at, picked_at) <= unixepoch()",
    )
    .fetch_one(db)
    .await
}
