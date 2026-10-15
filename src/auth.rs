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
    http::HeaderMap,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::{cookie::Cookie, SignedCookieJar};

use crate::AppState;

pub const COOKIE: &str = "joy_auth";
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
