use std::net::SocketAddr;

use askama::Template;
use axum::{
    extract::{ConnectInfo, State},
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
    jar: SignedCookieJar,
    Form(f): Form<LoginForm>,
) -> Result<Response, AppError> {
    let ip = addr.ip();
    if s.limiter.blocked(ip) {
        return Ok(render(LoginTpl::new(&s, "Too many tries. Wait 15 minutes."))?.into_response());
    }
    if !auth::verify(f.passcode.trim(), &s.cfg.passcode_hash) {
        s.limiter.fail(ip);
        return Ok(render(LoginTpl::new(&s, "That's not it."))?.into_response());
    }
    s.limiter.clear(ip);
    Ok((jar.add(auth::login_cookie(s.cfg.cookie_secure)), Redirect::to("/")).into_response())
}

pub async fn logout(jar: SignedCookieJar) -> impl IntoResponse {
    (jar.remove(Cookie::build(auth::COOKIE).path("/")), Redirect::to("/login"))
}
