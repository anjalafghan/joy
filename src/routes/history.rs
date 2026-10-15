use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::Form;
use serde::Deserialize;

use super::{render, AppError, HtmlResult};
use crate::{
    auth,
    db::{self, HistoryRow},
    AppState,
};

pub struct Month {
    pub label: String,
    pub items: Vec<HistoryRow>,
}

#[derive(Template)]
#[template(path = "history.html")]
struct HistoryTpl {
    /// Kept dates, newest month first.
    months: Vec<Month>,
    kept: usize,
    skipped: Vec<HistoryRow>,
}

pub async fn page(State(s): State<AppState>) -> HtmlResult {
    let (done, mut skipped): (Vec<_>, Vec<_>) =
        db::history(&s.db).await?.into_iter().partition(|r| r.outcome == "did_it");
    for row in &mut skipped {
        row.title = s.cfg.personalize(&row.title);
    }
    let kept = done.len();
    let mut months: Vec<Month> = vec![];
    for mut row in done {
        row.title = s.cfg.personalize(&row.title);
        let label = row.month_label();
        match months.last_mut() {
            Some(m) if m.label == label => m.items.push(row),
            _ => months.push(Month { label, items: vec![row] }),
        }
    }
    render(HistoryTpl { months, kept, skipped })
}

#[derive(Deserialize)]
pub struct RateForm {
    #[serde(default)]
    rating: String,
    #[serde(default)]
    note: String,
}

pub async fn rate(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    headers: HeaderMap,
    Form(f): Form<RateForm>,
) -> Result<Response, AppError> {
    db::rate(&s.db, id, f.rating.parse().ok(), &f.note).await?;
    // htmx saves in place and needs no page back; plain forms land back on their slip.
    Ok(if auth::is_htmx(&headers) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        Redirect::to(&format!("/history#k{id}")).into_response()
    })
}
