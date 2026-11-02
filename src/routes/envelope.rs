//! The envelope: what we send each other on WhatsApp is a link to a page in Joy, where the
//! date waits in a sealed envelope to be opened. The link carries a long random token and is
//! the only way in, so it opens without logging in (WhatsApp's browser doesn't share Joy's
//! login). A surprise stays a surprise: its envelope holds only the hints.

use askama::Template;
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use super::{dayplan::StepView, render, url_encode, ActivityView, AppError};
use crate::{db, AppState};

#[derive(Template)]
#[template(path = "envelope.html")]
struct EnvelopeTpl {
    /// "Anjal" when we know who sent it, else empty.
    from: String,
    /// "Diya", whoever it's for, else empty.
    to: String,
    /// What the letter says first: "It's a date", "A surprise", "What about this?".
    kicker: String,
    /// The slip's title, or "A surprise" when it's sealed.
    title: String,
    /// "Sat 7 Nov, 11:00 am", or empty.
    when: String,
    place: String,
    maps_url: Option<String>,
    /// "₹1,200 · 1–2 hours (about 5 h door to door)", or empty when sealed.
    meta: String,
    notes: String,
    steps: Vec<StepView>,
    /// A sealed surprise: the letter holds hints, not the date.
    sealed: bool,
    hours_note: String,
}

/// `GET /e/{token}`: the envelope.
pub async fn page(State(s): State<AppState>, Path(token): Path<String>) -> Result<Response, AppError> {
    let Some(env) = db::envelope(&s.db, &token).await? else {
        return Ok((StatusCode::NOT_FOUND, "This envelope isn't here any more.").into_response());
    };
    let learned = super::learned(&s).await?;
    let name = |i: Option<i64>| i.and_then(|i| s.cfg.people.get(i as usize)).map(|p| p.0.clone()).unwrap_or_default();
    let tpl = match env {
        db::Envelope::Slip(a) => {
            let v = ActivityView::new(&s.cfg, &a, &learned);
            EnvelopeTpl {
                from: String::new(),
                to: String::new(),
                kicker: "What about this?".into(),
                meta: meta(&v),
                title: v.title,
                when: String::new(),
                place: v.place,
                maps_url: v.maps_url,
                notes: v.notes,
                steps: vec![],
                sealed: false,
                hours_note: String::new(),
            }
        }
        db::Envelope::Date(row) => {
            // It's addressed to whoever didn't draw it: a surprise is sealed for them.
            let to = row.drawn_by.map(|d| (1 - d.clamp(0, 1)) as usize);
            let plan = super::dayplan::build(&s, &row, to).await?;
            let sealed = super::decide::sealed_for(&row, to);
            let a = db::get(&s.db, row.activity_id).await?;
            let v = a.as_ref().map(|a| ActivityView::new(&s.cfg, a, &learned));
            let when = match (&plan, row.planned_local.as_deref()) {
                (Some(p), _) => p.when.clone(),
                (None, Some(l)) => super::planned_label(l),
                _ => String::new(),
            };
            let (steps, hours_note) = plan.map(|p| (p.steps, p.hours_note)).unwrap_or_default();
            EnvelopeTpl {
                from: name(row.drawn_by),
                to: name(to.map(|t| t as i64)),
                kicker: if sealed { "A surprise".into() } else { "It's a date".into() },
                title: if sealed { "Something's planned".into() } else { s.cfg.personalize(&row.title) },
                when,
                place: if sealed { String::new() } else { v.as_ref().map(|v| v.place.clone()).unwrap_or_default() },
                maps_url: if sealed { None } else { v.as_ref().and_then(|v| v.maps_url.clone()) },
                meta: if sealed { String::new() } else { v.as_ref().map(meta).unwrap_or_default() },
                notes: if sealed { String::new() } else { v.map(|v| v.notes).unwrap_or_default() },
                steps,
                sealed,
                hours_note,
            }
        }
    };
    let html = render(tpl)?;
    // Only those of us with the link: keep it out of search and out of other sites' logs.
    Ok(([(header::HeaderName::from_static("x-robots-tag"), "noindex"), (header::REFERRER_POLICY, "no-referrer")], html).into_response())
}

fn meta(v: &ActivityView) -> String {
    let time = if v.door.is_empty() { v.duration.clone() } else { format!("{} ({})", v.duration, v.door) };
    format!("{} · {time}", v.cost)
}

/// The WhatsApp message for an envelope: a line and the link, nothing that gives it away.
pub fn wa_text(cfg: &crate::config::Config, token: &str, line: &str) -> String {
    format!("{line}\n{}/e/{token}", cfg.public_url)
}

pub fn wa_url(text: &str) -> String {
    format!("https://wa.me/?text={}", url_encode(text))
}
