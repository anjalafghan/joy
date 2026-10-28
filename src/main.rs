mod auth;
mod config;
mod db;
mod ics;
mod photos;
mod picker;
mod routes;
#[cfg(test)]
mod route_tests;
mod travel;
mod weather;
mod web;

use std::{io::Write, net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::{DefaultBodyLimit, FromRef},
    middleware,
    routing::{get, post},
    Router,
};
use axum_extra::extract::cookie::Key;
use rand::{distributions::Alphanumeric, Rng};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tower_http::{services::ServeDir, trace::TraceLayer};

use config::Config;

/// Short hash of static/ + templates/: versions asset URLs and the service worker.
pub const BUILD: &str = env!("JOY_BUILD");

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cfg: Arc<Config>,
    pub limiter: Arc<auth::Limiter>,
    pub weather: Arc<weather::Weather>,
}

impl FromRef<AppState> for Key {
    fn from_ref(s: &AppState) -> Key {
        s.cfg.key.clone()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    if std::env::args().nth(1).as_deref() == Some("hash-passcode") {
        return hash_passcode();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "joy=info,tower_http=warn".into()),
        )
        .init();

    let cfg = Config::from_env()?;
    if cfg.dev_no_login {
        tracing::warn!("JOY_DEV_NO_LOGIN=1: the passcode is OFF. Never use this outside local design work.");
    }
    if let Some(dir) = sqlite_dir(&cfg.database_url) {
        std::fs::create_dir_all(dir).context("creating database directory")?;
    }
    let db = SqlitePoolOptions::new().connect(&cfg.database_url).await.context("opening database")?;
    sqlx::migrate!().run(&db).await.context("running migrations")?;

    // `joy forget-passcode 0|1`: for when one of us forgets their own passcode. Run it on the
    // server (fly ssh console -C "joy forget-passcode 0"); that person sets a new one next time.
    if std::env::args().nth(1).as_deref() == Some("forget-passcode") {
        let i: usize = std::env::args().nth(2).and_then(|a| a.parse().ok()).filter(|i| *i < 2).context("usage: joy forget-passcode 0|1")?;
        db::set_setting(&db, &auth::pin_key(i), None).await?;
        db::set_setting(&db, &auth::token_key(i), None).await?;
        println!("{} can set a new passcode now (and is signed out everywhere).", cfg.people[i].0);
        return Ok(());
    }

    let addr = SocketAddr::from((cfg.host, cfg.port));
    std::fs::create_dir_all(&cfg.photos_dir).context("creating photos directory")?;
    let fake_rain = std::env::var("WEATHER_FAKE_PERCENT").ok().and_then(|v| v.parse().ok());
    let weather = Arc::new(weather::Weather::new(weather::midpoint([cfg.people[0].1, cfg.people[1].1]), fake_rain));
    let state = AppState { db, cfg: Arc::new(cfg), limiter: Default::default(), weather };

    let app = build_app(state);

    tracing::info!("Joy is listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    Ok(())
}

/// Every route, wired to `state`. Shared by `main` and the route tests.
pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(routes::decide::page))
        .route("/pick", post(routes::decide::pick))
        .route("/middle", post(routes::middle::check_in))
        .route("/match", post(routes::decide::matching))
        .route("/accept/{id}", post(routes::decide::accept))
        .route("/wishlist", get(routes::wishlist::page).post(routes::wishlist::create))
        .route("/wishlist/new", get(routes::wishlist::new))
        .route("/wishlist/{id}", get(routes::wishlist::edit).post(routes::wishlist::update))
        .route("/wishlist/{id}/archive", post(routes::wishlist::archive))
        .route("/wishlist/{id}/restore", post(routes::wishlist::restore))
        .route("/wishlist/{id}/delete", post(routes::wishlist::delete))
        .route("/wishlist/{id}/unveto", post(routes::wishlist::unveto))
        .route("/history", get(routes::history::page))
        .route("/year", get(routes::year::current))
        .route("/year/{year}", get(routes::year::page))
        .route("/year/{year}/{month}", get(routes::month::page))
        .route("/history/when", post(routes::history::when))
        .route("/history/{id}/invite.ics", get(routes::history::invite))
        .route("/history/{id}/reveal", post(routes::history::reveal))
        .route("/history/{id}/delete", post(routes::history::delete))
        .route("/history/{id}/cost", post(routes::history::set_cost))
        .route(
            "/history/{id}/photo",
            post(routes::history::upload_photo).layer(DefaultBodyLimit::max(photos::MAX_UPLOAD_BYTES + 64 * 1024)),
        )
        .route("/history/{id}/photo/delete", post(routes::history::delete_photo))
        .route("/photos/{id}", get(routes::history::photo))
        .route("/budget", post(routes::history::set_budget))
        .route("/budget/rollover", post(routes::history::rollover))
        .route("/budget/spend", post(routes::history::add_spend))
        .route("/budget/spend/{id}/delete", post(routes::history::delete_spend))
        .route("/who", get(routes::auth::who_page).post(routes::auth::set_who))
        .route("/history/{id}", post(routes::history::rate))
        .route("/logout", post(routes::auth::logout))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_login))
        .route("/login", get(routes::auth::page).post(routes::auth::login))
        // Reachable without logging in: the worker must update even after the login cookie expires.
        .route("/sw.js", get(web::service_worker))
        .route("/offline", get(web::offline))
        .nest_service(
            "/static",
            tower::ServiceBuilder::new().layer(middleware::from_fn(web::static_cache)).service(ServeDir::new("static")),
        )
        .layer(middleware::from_fn_with_state(state.clone(), web::page_headers))
        // Outermost, so everything inside sees only a who cookie backed by that person's passcode.
        .layer(middleware::from_fn_with_state(state.clone(), auth::check_who))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// `joy hash-passcode`: reads the shared passcode from stdin and prints .env lines.
fn hash_passcode() -> anyhow::Result<()> {
    eprint!("Shared passcode: ");
    std::io::stderr().flush()?;
    let mut pass = String::new();
    std::io::stdin().read_line(&mut pass)?;
    let pass = pass.trim();
    anyhow::ensure!(pass.len() >= 4, "use at least 4 characters");
    let secret: String = rand::thread_rng().sample_iter(&Alphanumeric).take(64).map(char::from).collect();
    println!("APP_PASSCODE_HASH='{}'", auth::hash(pass));
    println!("SESSION_SECRET={secret}");
    Ok(())
}

pub(crate) fn sqlite_dir(url: &str) -> Option<&std::path::Path> {
    let path = url.strip_prefix("sqlite://").or_else(|| url.strip_prefix("sqlite:"))?;
    let path = path.split('?').next()?;
    std::path::Path::new(path).parent().filter(|p| !p.as_os_str().is_empty())
}
