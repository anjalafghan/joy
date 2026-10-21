use std::{
    collections::HashMap,
    net::IpAddr,
    sync::Mutex,
    time::{Duration, Instant},
};

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{cookie::Cookie, SignedCookieJar};

use crate::AppState;

pub const COOKIE: &str = "joy_auth";
/// Which of us is holding this device, as "<0|1>.<token>". Getting it takes that person's own
/// passcode (on top of the shared one), so one of us can't switch to the other and read their
/// surprises. The token changes whenever that person sets a new passcode, which signs their
/// name out everywhere else. `check_who` drops a cookie whose token no longer matches.
pub const WHO_COOKIE: &str = "joy_who";
/// At least this many characters in a personal passcode.
pub const MIN_PERSONAL: usize = 4;

pub fn who(jar: &SignedCookieJar) -> Option<usize> {
    let c = jar.get(WHO_COOKIE)?;
    let (i, token) = c.value().split_once('.')?;
    i.parse().ok().filter(|i| *i < 2 && !token.is_empty())
}

pub fn who_cookie(i: usize, token: &str, secure: bool) -> Cookie<'static> {
    Cookie::build((WHO_COOKIE, format!("{i}.{token}")))
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(365))
        .build()
}

pub fn pin_key(i: usize) -> String {
    format!("person_pin_{i}")
}

pub fn token_key(i: usize) -> String {
    format!("person_token_{i}")
}

pub fn new_token() -> String {
    use rand::Rng;
    let bytes: [u8; 16] = rand::thread_rng().gen();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Forgets a who cookie that isn't (or is no longer) backed by that person's passcode: old
/// "0"/"1" cookies from before personal passcodes, and cookies from before a passcode change.
/// Handlers then see nobody, so surprises stay sealed (fail closed).
pub async fn check_who(State(s): State<AppState>, jar: SignedCookieJar, mut req: Request, next: Next) -> Response {
    let Some(c) = jar.get(WHO_COOKIE) else { return next.run(req).await };
    let valid = match c.value().split_once('.') {
        Some((i, token)) => match i.parse::<usize>() {
            Ok(i) if i < 2 && !token.is_empty() => {
                crate::db::setting(&s.db, &token_key(i)).await.ok().flatten().is_some_and(|t| t == token)
            }
            _ => false,
        },
        None => false,
    };
    if valid {
        return next.run(req).await;
    }
    let kept: Vec<String> = req
        .headers()
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .map(str::trim)
        .filter(|p| !p.is_empty() && !p.starts_with(&format!("{WHO_COOKIE}=")))
        .map(String::from)
        .collect();
    req.headers_mut().remove(header::COOKIE);
    if let Ok(v) = HeaderValue::from_str(&kept.join("; ")) {
        req.headers_mut().insert(header::COOKIE, v);
    }
    let mut res = next.run(req).await;
    let sets_new = res.headers().get_all(header::SET_COOKIE).iter().any(|v| v.to_str().is_ok_and(|v| v.starts_with(&format!("{WHO_COOKIE}="))));
    if !sets_new {
        let gone = if s.cfg.cookie_secure { "joy_who=; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax" } else { "joy_who=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax" };
        res.headers_mut().append(header::SET_COOKIE, HeaderValue::from_static(gone));
    }
    res
}

const MAX_FAILURES: u32 = 10;
const WINDOW: Duration = Duration::from_secs(15 * 60);

pub fn hash(passcode: &str) -> String {
    Argon2::default()
        .hash_password(passcode.as_bytes(), &SaltString::generate(&mut OsRng))
        .expect("argon2 hashing")
        .to_string()
}

pub fn verify(passcode: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(passcode.as_bytes(), &h).is_ok())
}

pub fn login_cookie(secure: bool) -> Cookie<'static> {
    Cookie::build((COOKIE, "ok"))
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(time::Duration::days(30))
        .build()
}

pub async fn require_login(
    State(s): State<AppState>,
    jar: SignedCookieJar,
    req: Request,
    next: Next,
) -> Response {
    if s.cfg.dev_no_login || jar.get(COOKIE).is_some_and(|c| c.value() == "ok") {
        return next.run(req).await;
    }
    if is_htmx(req.headers()) {
        return ([("HX-Redirect", "/login")], "").into_response();
    }
    Redirect::to("/login").into_response()
}

pub fn is_htmx(h: &HeaderMap) -> bool {
    h.contains_key("HX-Request")
}

/// Counts failed logins per IP so the passcode can't be brute-forced.
#[derive(Default)]
pub struct Limiter(Mutex<HashMap<IpAddr, (u32, Instant)>>);

impl Limiter {
    pub fn blocked(&self, ip: IpAddr) -> bool {
        let map = self.0.lock().unwrap();
        map.get(&ip).is_some_and(|(n, since)| *n >= MAX_FAILURES && since.elapsed() < WINDOW)
    }

    pub fn fail(&self, ip: IpAddr) {
        let mut map = self.0.lock().unwrap();
        let entry = map.entry(ip).or_insert((0, Instant::now()));
        if entry.1.elapsed() >= WINDOW {
            *entry = (0, Instant::now());
        }
        entry.0 += 1;
    }

    pub fn clear(&self, ip: IpAddr) {
        self.0.lock().unwrap().remove(&ip);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_roundtrip() {
        let h = hash("pikachu");
        assert!(verify("pikachu", &h));
        assert!(!verify("mudkip", &h));
        assert!(!verify("pikachu", "garbage"));
    }

    #[test]
    fn limiter_blocks_after_max_failures() {
        let l = Limiter::default();
        let ip: IpAddr = "10.0.0.1".parse().unwrap();
        for _ in 0..MAX_FAILURES {
            assert!(!l.blocked(ip));
            l.fail(ip);
        }
        assert!(l.blocked(ip));
        l.clear(ip);
        assert!(!l.blocked(ip));
    }
}
