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

    pub fn candidate(&self, distance_km: Option<f64>) -> Candidate {
        Candidate {
            id: self.id,
            title: self.title.clone(),
            est_cost: self.est_cost,
            setting: self.setting.clone(),
            duration: self.duration(),
            tags: self.tag_list(),
            energy: self.energy.clone(),
            last_done_at: self.last_done_at,
            last_skipped_at: self.last_skipped_at,
            distance_km,
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
                    added_by, times_done, last_done_at, last_skipped_at, generated";

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
        None => "INSERT INTO activities (title, notes, est_cost, setting, duration, venue, area, lat, lng, tags, energy, added_by, id)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        Some(_) => "UPDATE activities SET title = ?, notes = ?, est_cost = ?, setting = ?, duration = ?, venue = ?, area = ?,
                    lat = ?, lng = ?, tags = ?, energy = ?, added_by = ? WHERE id = ?",
    };
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
    a.duration, a.energy, a.tags, COALESCE(h.est_cost, a.est_cost) AS est_cost, h.actual_cost, a.lat, a.lng,
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
        sqlx::query("UPDATE activities SET est_cost = ? WHERE id = (SELECT activity_id FROM history WHERE id = ?)")
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

/// What this calendar month's kept dates add up to (local time): the real cost where
/// we filled it in, the estimate otherwise.
pub async fn spent_this_month(db: &SqlitePool) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(COALESCE(h.actual_cost, h.est_cost, a.est_cost)), 0) FROM history h JOIN activities a ON a.id = h.activity_id
         WHERE h.outcome = 'did_it'
           AND strftime('%Y-%m', h.picked_at, 'unixepoch', 'localtime') = strftime('%Y-%m', 'now', 'localtime')",
    )
    .fetch_one(db)
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
