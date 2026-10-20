//! Cross-cutting HTTP: caching rules, privacy headers, the service worker and the offline page.
//! See docs/year-in-review-and-offline.md (sections 4 and 5).

use askama::Template;
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue},
    middleware::Next,
    response::{IntoResponse, Response},
};
use axum_extra::extract::SignedCookieJar;

use crate::{auth, routes::HtmlResult, AppState, BUILD};

/// `/static/*`: forever-cacheable when the URL carries this build's `?v=`, else revalidate.
pub async fn static_cache(req: Request, next: Next) -> Response {
    let versioned = req.uri().query().is_some_and(|q| q.split('&').any(|p| p == format!("v={BUILD}")));
    let mut res = next.run(req).await;
    let value = if versioned && res.status().is_success() { "public, max-age=31536000, immutable" } else { "no-cache" };
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    res
}

/// Every response: privacy headers. HTML pages: never in the browser's HTTP cache (the service
/// worker keeps its own, purgeable copy), tagged with who it was made for and the build.
pub async fn page_headers(State(s): State<AppState>, jar: SignedCookieJar, req: Request, next: Next) -> Response {
    let viewer = match auth::who(&jar) {
        Some(0) => "0",
        Some(1) => "1",
        _ => "none",
    };
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    // Maps, Uber and WhatsApp links shouldn't tell those sites where they were opened from.
    h.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert("cross-origin-opener-policy", HeaderValue::from_static("same-origin"));
    h.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=(), usb=()"),
    );
    if s.cfg.cookie_secure {
        h.insert("strict-transport-security", HeaderValue::from_static("max-age=31536000"));
    }
    let is_html = h.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("text/html"));
    if is_html {
        h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        h.insert("x-joy-viewer", HeaderValue::from_static(viewer));
        if let Ok(v) = HeaderValue::from_str(BUILD) {
            h.insert("x-joy-build", v);
        }
    }
    res
}

/// The service worker, versioned with this build. Served at the root so it can control every page,
/// and outside the login wall so an update never turns into a redirect.
pub async fn service_worker() -> Response {
    let body = include_str!("../static/sw.js").replace("__BUILD__", BUILD);
    (
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        body,
    )
        .into_response()
}

#[derive(Template)]
#[template(path = "offline.html")]
struct OfflineTpl;

/// Shown by the service worker when there's no connection and no saved copy. No personal data.
pub async fn offline() -> HtmlResult {
    crate::routes::render(OfflineTpl)
}
