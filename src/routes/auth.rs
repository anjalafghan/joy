use std::net::SocketAddr;

use askama::Template;
use axum::{
    extract::{ConnectInfo, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{cookie::Cookie, Form, SignedCookieJar};
use serde::Deserialize;

use super::{render, AppError, HtmlResult};
use crate::{auth, AppState};

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTpl {
    error: String,
    /// "R+N" for the tag on the jar.
    initials: String,
}

impl LoginTpl {
    fn new(s: &AppState, error: &str) -> Self {
        let initial = |i: usize| s.cfg.people[i].0.chars().next().unwrap_or('?').to_uppercase().to_string();
        Self { error: error.into(), initials: format!("{}+{}", initial(0), initial(1)) }
    }
}

pub async fn page(State(s): State<AppState>) -> HtmlResult {
    render(LoginTpl::new(&s, ""))
}

#[derive(Deserialize)]
pub struct LoginForm {
    passcode: String,
}

pub async fn login(
    State(s): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: SignedCookieJar,
    Form(f): Form<LoginForm>,
) -> Result<Response, AppError> {
    let fly_ip = || headers.get("fly-client-ip")?.to_str().ok()?.parse().ok();
    let ip = if s.cfg.trust_fly_client_ip { fly_ip().unwrap_or(addr.ip()) } else { addr.ip() };
    if s.limiter.blocked(ip) {
        return Ok(render(LoginTpl::new(&s, "Too many tries. Wait 15 minutes."))?.into_response());
    }
    if !auth::verify(f.passcode.trim(), &s.cfg.passcode_hash) {
        s.limiter.fail(ip);
        return Ok(render(LoginTpl::new(&s, "That's not it."))?.into_response());
    }
    s.limiter.clear(ip);
    // First time on this device: ask who's holding it.
    let next = if auth::who(&jar).is_some() { "/" } else { "/who" };
    Ok((jar.add(auth::login_cookie(s.cfg.cookie_secure)), Redirect::to(next)).into_response())
}

#[derive(Template)]
#[template(path = "who.html")]
struct WhoTpl {
    /// (value, name, is the current choice)
    people: Vec<(usize, String, bool)>,
    /// Whether this device has chosen before (switching vs first time).
    switching: bool,
}

pub async fn who_page(State(s): State<AppState>, jar: SignedCookieJar) -> HtmlResult {
    let who = auth::who(&jar);
    render(WhoTpl {
        people: s.cfg.people.iter().enumerate().map(|(i, (n, _))| (i, n.clone(), who == Some(i))).collect(),
        switching: who.is_some(),
    })
}

#[derive(Deserialize)]
pub struct WhoForm {
    who: usize,
}

pub async fn set_who(State(s): State<AppState>, jar: SignedCookieJar, Form(f): Form<WhoForm>) -> Response {
    if f.who > 1 {
        return Redirect::to("/who").into_response();
    }
    (jar.add(auth::who_cookie(f.who, s.cfg.cookie_secure)), Redirect::to("/")).into_response()
}

pub async fn logout(jar: SignedCookieJar) -> impl IntoResponse {
    (jar.remove(Cookie::build(auth::COOKIE).path("/")), Redirect::to("/login"))
}
