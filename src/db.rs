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
                    added_by, times_done, last_done_at, last_skipped_at";

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

pub async fn save(db: &SqlitePool, id: Option<i64>, a: &ActivityInput, latlng: Option<(f64, f64)>) -> sqlx::Result<()> {
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
    sqlx::query(sql)
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
    Ok(())
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

/// Accept a pick. A double-tap within a minute only counts once.
pub async fn accept(db: &SqlitePool, id: i64) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    // One statement, so two concurrent taps can't both pass the "recent?" check.
    let inserted = sqlx::query(
        "INSERT INTO history (activity_id, outcome) SELECT ?1, 'did_it'
         WHERE NOT EXISTS (SELECT 1 FROM history WHERE activity_id = ?1 AND outcome = 'did_it'
                           AND picked_at > unixepoch() - 60)",
    )
    .bind(id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 1 {
        sqlx::query("UPDATE activities SET times_done = times_done + 1, last_done_at = unixepoch() WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await
}

/// Today's local date for the "it's a date" stamp, e.g. "14 · 10 · 2026".
pub async fn today(db: &SqlitePool) -> sqlx::Result<String> {
    sqlx::query_scalar("SELECT strftime('%d · %m · %Y', 'now', 'localtime')").fetch_one(db).await
}

#[derive(Debug, FromRow)]
pub struct HistoryRow {
    pub id: i64,
    pub title: String,
    pub setting: String,
    pub place: String,
    /// "YYYY-MM-DD"
    pub day: String,
    pub outcome: String,
    pub rating: Option<i64>,
    pub note: String,
}

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November",
    "December",
];

impl HistoryRow {
    pub fn is_rated(&self, n: &i64) -> bool {
        self.rating == Some(*n)
    }

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

pub async fn history(db: &SqlitePool) -> sqlx::Result<Vec<HistoryRow>> {
    sqlx::query_as(
        "SELECT h.id, a.title, a.setting,
                CASE WHEN a.venue != '' THEN a.venue ELSE a.area END AS place,
                date(h.picked_at, 'unixepoch', 'localtime') AS day, h.outcome, h.rating, h.note
         FROM history h JOIN activities a ON a.id = h.activity_id
         ORDER BY h.picked_at DESC, h.id DESC LIMIT 300",
    )
    .fetch_all(db)
    .await
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
