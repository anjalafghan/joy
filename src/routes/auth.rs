use std::net::SocketAddr;

use askama::Template;
use axum::{
    extract::{ConnectInfo, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{cookie::Cookie, Form, SignedCookieJar};
use serde::Deserialize;

use super::{render, AppError, HtmlResult};
use crate::{auth, db, AppState};

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
    let ip = client_ip(&s, &addr, &headers);
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
    /// (value, name, is the current choice, has a personal passcode)
    people: Vec<(usize, String, bool, bool)>,
    /// Whether this device has chosen before (switching vs first time).
    switching: bool,
    /// "choose" | "unlock" (enter your passcode) | "set" (first time: make one) | "change".
    mode: &'static str,
    /// The person being unlocked, set up or changed.
    target: usize,
    target_name: String,
    partner_name: String,
    error: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct WhoQuery {
    #[serde(rename = "as")]
    as_: Option<usize>,
    change: Option<String>,
}

async fn has_pin(s: &AppState, i: usize) -> Result<bool, AppError> {
    Ok(db::setting(&s.db, &auth::pin_key(i)).await?.is_some())
}

async fn who_tpl(s: &AppState, jar: &SignedCookieJar, target: Option<usize>, change: bool, error: &str) -> Result<WhoTpl, AppError> {
    let who = auth::who(jar);
    let mut people = vec![];
    for (i, (n, _)) in s.cfg.people.iter().enumerate() {
        people.push((i, n.clone(), who == Some(i), has_pin(s, i).await?));
    }
    let target = target.filter(|t| *t < 2);
    let mode = match target {
        None => "choose",
        Some(t) if change && who == Some(t) && people[t].3 => "change",
        Some(t) if people[t].3 => "unlock",
        Some(_) => "set",
    };
    let t = target.unwrap_or(0);
    Ok(WhoTpl {
        switching: who.is_some(),
        mode,
        target: t,
        target_name: s.cfg.people[t].0.clone(),
        partner_name: s.cfg.people[1 - t].0.clone(),
        error: error.into(),
        people,
    })
}

pub async fn who_page(State(s): State<AppState>, jar: SignedCookieJar, Query(q): Query<WhoQuery>) -> HtmlResult {
    render(who_tpl(&s, &jar, q.as_, q.change.is_some(), "").await?)
}

#[derive(Deserialize)]
pub struct WhoForm {
    who: usize,
    #[serde(default)]
    pin: String,
    /// Typed again, when setting a passcode for the first time.
    #[serde(default)]
    pin2: String,
    /// Changing it: the new one, twice.
    #[serde(default)]
    new_pin: String,
    #[serde(default)]
    new_pin2: String,
}

/// "I'm Rohit": with Rohit's own passcode, or setting it if he hasn't yet. Changing it also
/// gives him a new token, so his name is signed out on every other device.
pub async fn set_who(
    State(s): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    jar: SignedCookieJar,
    Form(f): Form<WhoForm>,
) -> Result<Response, AppError> {
    if f.who > 1 {
        return Ok(Redirect::to("/who").into_response());
    }
    let ip = client_ip(&s, &addr, &headers);
    let again = |msg: &'static str, change: bool| {
        let (s, jar) = (s.clone(), jar.clone());
        async move { Ok::<Response, AppError>(render(who_tpl(&s, &jar, Some(f.who), change, msg).await?)?.into_response()) }
    };
    if s.limiter.blocked(ip) {
        return again("Too many tries. Wait 15 minutes.", false).await;
    }
    let stored = db::setting(&s.db, &auth::pin_key(f.who)).await?;
    let token = match stored {
        Some(hash) => {
            if !auth::verify(f.pin.trim(), &hash) {
                s.limiter.fail(ip);
                tracing::info!(who = f.who, "who's holding the phone: wrong personal passcode");
                return again(if f.new_pin.is_empty() { "That's not it." } else { "Your current passcode isn't right." }, !f.new_pin.is_empty()).await;
            }
            s.limiter.clear(ip);
            if f.new_pin.is_empty() {
                db::setting(&s.db, &auth::token_key(f.who)).await?.unwrap_or_else(auth::new_token)
            } else {
                let new = f.new_pin.trim();
                if new.chars().count() < auth::MIN_PERSONAL {
                    return again("Use at least 4 characters.", true).await;
                }
                if new != f.new_pin2.trim() {
                    return again("The new passcodes don't match.", true).await;
                }
                save_pin(&s, f.who, new).await?
            }
        }
        None => {
            let pin = f.pin.trim();
            if pin.chars().count() < auth::MIN_PERSONAL {
                return again("Use at least 4 characters.", false).await;
            }
            if pin != f.pin2.trim() {
                return again("Those don't match. Type the same passcode twice.", false).await;
            }
            save_pin(&s, f.who, pin).await?
        }
    };
    tracing::info!(who = f.who, "who's holding the phone: switched");
    // No Clear-Site-Data here: browsers can stall the redirect for a long time while they clear
    // their cache. Nothing personal is in it anyway (pages are no-store), and the service
    // worker deletes its saved copies itself on this POST.
    Ok((jar.add(auth::who_cookie(f.who, &token, s.cfg.cookie_secure)), Redirect::to("/")).into_response())
}

async fn save_pin(s: &AppState, who: usize, pin: &str) -> Result<String, AppError> {
    let token = auth::new_token();
    db::set_setting(&s.db, &auth::pin_key(who), Some(&auth::hash(pin))).await?;
    db::set_setting(&s.db, &auth::token_key(who), Some(&token)).await?;
    Ok(token)
}

fn client_ip(s: &AppState, addr: &SocketAddr, headers: &HeaderMap) -> std::net::IpAddr {
    let fly_ip = || headers.get("fly-client-ip")?.to_str().ok()?.parse().ok();
    if s.cfg.trust_fly_client_ip { fly_ip().unwrap_or(addr.ip()) } else { addr.ip() }
}

/// Logging out also forgets who was holding the phone, and asks the browser to drop joy's
/// cached pages and storage (the service worker purges its own copies too).
pub async fn logout(jar: SignedCookieJar) -> impl IntoResponse {
    let jar = jar.remove(Cookie::build(auth::COOKIE).path("/")).remove(Cookie::build(auth::WHO_COOKIE).path("/"));
    // "storage" drops the service worker and its saved copies; not "cache", which can stall.
    (jar, [("clear-site-data", r#""storage""#)], Redirect::to("/login"))
}
