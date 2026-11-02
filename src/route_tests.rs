//! Route tests: the real router against an in-memory database, for the rules that
//! matter most (surprises stay sealed, peeks are capped on the server, double taps
//! don't double-record, the jar skips what it should).

use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    extract::connect_info::MockConnectInfo,
    http::{header, HeaderMap, Method, Request, StatusCode},
    Router,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tower::ServiceExt;

use crate::{auth, build_app, config::Config, weather::Weather, AppState};

pub struct T {
    pub app: Router,
    pub db: SqlitePool,
    pub photos: std::path::PathBuf,
}

pub struct Res {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

impl Res {
    pub fn location(&self) -> &str {
        self.headers.get(header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or("")
    }

    /// Value of a hidden input like `name="used" value="2"`.
    pub fn hidden(&self, name: &str) -> String {
        let key = format!("name=\"{name}\" value=\"");
        self.body.split(&key).nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string()
    }
}

pub async fn setup_with(dev_no_login: bool, passcode_hash: &str) -> T {
    setup_custom(dev_no_login, passcode_hash, |_| {}).await
}

pub async fn setup_custom(dev_no_login: bool, passcode_hash: &str, tweak: impl FnOnce(&mut Config)) -> T {
    // One connection: every in-memory SQLite connection is its own empty database.
    let db = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!().run(&db).await.unwrap();
    let photos = std::env::temp_dir().join(format!("joy-test-{}", rand::random::<u64>()));
    std::fs::create_dir_all(&photos).unwrap();
    let mut cfg = Config::for_tests(photos.clone(), passcode_hash, dev_no_login);
    tweak(&mut cfg);
    let state = AppState {
        db: db.clone(),
        cfg: Arc::new(cfg),
        limiter: Default::default(),
        // A fixed "no rain" forecast: tests never touch the network.
        weather: Arc::new(Weather::new(None, Some(0))),
    };
    let app = build_app(state).layer(MockConnectInfo(SocketAddr::from(([127, 0, 0, 1], 9))));
    T { app, db, photos }
}

pub async fn setup() -> T {
    setup_with(true, "unused").await
}

impl T {
    pub async fn send(&self, method: Method, uri: &str, cookie: &str, form: Option<&str>, htmx: bool) -> Res {
        let mut b = Request::builder().method(method).uri(uri);
        if !cookie.is_empty() {
            b = b.header(header::COOKIE, cookie);
        }
        if htmx {
            b = b.header("HX-Request", "true");
        }
        let req = match form {
            Some(f) => b.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Body::from(f.to_string())),
            None => b.body(Body::empty()),
        }
        .unwrap();
        let res = self.app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
        Res { status, headers, body: String::from_utf8_lossy(&bytes).into_owned() }
    }

    pub async fn get(&self, uri: &str, cookie: &str) -> Res {
        self.send(Method::GET, uri, cookie, None, false).await
    }

    pub async fn post(&self, uri: &str, cookie: &str, form: &str) -> Res {
        self.send(Method::POST, uri, cookie, Some(form), true).await
    }

    /// The cookie a device gets after saying who's holding it (0 or 1), with that person's own
    /// passcode (set on first use).
    pub async fn as_person(&self, who: usize) -> String {
        let pin = format!("pin-of-{who}");
        let r = self.send(Method::POST, "/who", "", Some(&format!("who={who}&pin={pin}&pin2={pin}")), false).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER, "{}", r.body);
        cookies(&r.headers)
    }

    pub async fn id_of(&self, title: &str) -> i64 {
        sqlx::query_scalar("SELECT id FROM activities WHERE title = ?").bind(title).fetch_one(&self.db).await.unwrap()
    }

    pub async fn exec(&self, sql: &str) {
        sqlx::query(sql).execute(&self.db).await.unwrap();
    }

    pub async fn scalar(&self, sql: &str) -> i64 {
        sqlx::query_scalar(sql).fetch_one(&self.db).await.unwrap()
    }
}

/// "a=1; b=2" from every Set-Cookie.
pub fn cookies(h: &HeaderMap) -> String {
    h.get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok()?.split(';').next().map(str::to_string))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The jar page minus its `<script id="slips">` list of every slip.
pub fn without_slip_list(html: &str) -> String {
    match (html.find("id=\"slips\""), html.find("id=\"slips\"").and_then(|i| html[i..].find("</script>").map(|j| i + j))) {
        (Some(a), Some(b)) => format!("{}{}", &html[..a], &html[b..]),
        _ => html.to_string(),
    }
}

const PLAY: &str = "A play at Prithvi Theatre";

/// Rohit (0) keeps the play as a surprise for Neha (1), with a time and a photo.
async fn surprise_from_rohit(t: &T) -> i64 {
    let a = t.id_of(PLAY).await;
    t.exec(&format!(
        "INSERT INTO history (activity_id, outcome, drawn_by, surprise, planned_at, has_photo, est_cost)
         VALUES ({a}, 'did_it', 0, 1, unixepoch('now', '+2 days'), 1, 1200)"
    ))
    .await;
    let h = t.scalar("SELECT max(id) FROM history").await;
    std::fs::write(crate::photos::path(&t.photos, h), b"jpeg").unwrap();
    h
}

#[tokio::test]
async fn a_surprise_never_reaches_the_partner() {
    let t = setup().await;
    let h = surprise_from_rohit(&t).await;
    // The same slip was also put back once earlier, and kept a year ago.
    let a = t.id_of(PLAY).await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome) VALUES ({a}, 'skipped')")).await;
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);

    let kept = t.get("/history", &neha).await;
    assert_eq!(kept.status, StatusCode::OK);
    assert!(!kept.body.contains("Prithvi"), "Neha's Kept leaks the surprise");
    assert!(kept.body.contains("A surprise from Rohit"));
    assert!(t.get("/history", &rohit).await.body.contains("Prithvi"), "the planner sees their own surprise");
    assert!(!t.get("/history", "").await.body.contains("Prithvi"), "an unknown device sees nothing");

    let ics = t.get(&format!("/history/{h}/invite.ics"), &neha).await;
    assert!(ics.body.contains("SUMMARY:A surprise from Rohit") && !ics.body.contains("Prithvi"));
    let sealed_send = t.get(&format!("/history/{h}/invite.ics?sealed=1"), &rohit).await;
    assert!(!sealed_send.body.contains("Prithvi"), "the invite Rohit sends is sealed too");

    assert_eq!(t.get(&format!("/photos/{h}"), &neha).await.status, StatusCode::NOT_FOUND);
    assert_eq!(t.get(&format!("/photos/{h}"), &rohit).await.status, StatusCode::OK);

    for (uri, form) in [
        (format!("/history/{h}/cost"), "actual=10"),
        (format!("/history/{h}/delete"), ""),
        (format!("/history/{h}/photo/delete"), ""),
    ] {
        assert_eq!(t.post(&uri, &neha, form).await.status, StatusCode::FORBIDDEN, "{uri}");
    }

    // Every slip's title is in the jar's own slip list (the Slips page shows them anyway);
    // what must never name the surprise is everything else: nudges, memories.
    let jar = without_slip_list(&t.get("/", &neha).await.body);
    assert!(!jar.contains("Prithvi"), "the jar page (nudges, memories) leaks it");
}

#[tokio::test]
async fn whoever_s_turn_it_is_gets_a_bonus_peek_and_nobody_else_does() {
    let t = setup().await;
    // Generated slips go back for free; keep this test about the peek cap only.
    t.exec("UPDATE activities SET generated = 0").await;
    // Rohit kept the last date, so it's Neha's turn.
    let cook = t.id_of("Cook together").await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, drawn_by) VALUES ({cook}, 'did_it', 0)")).await;
    let current = t.id_of("Play Pokémon Emerald together").await;
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);
    let form = format!("used=3&current={current}&reroll=1&excluded=");

    let r = t.post("/pick", &neha, &form).await;
    assert!(r.body.contains("data-id=") && !r.body.contains(&format!("data-id=\"{current}\"")), "Neha's 4th peek draws a new slip");
    assert!(r.body.contains("No more peeking"), "and that was her last one");
    let r = t.post("/pick", &rohit, &form).await;
    assert!(r.body.contains(&format!("data-id=\"{current}\"")), "Rohit keeps the same slip");
    assert!(r.body.contains("No more peeking"), "Rohit is out of peeks");
}

#[tokio::test]
async fn a_double_tap_on_it_s_a_date_records_one_date() {
    let t = setup().await;
    let id = t.id_of("Cook together").await;
    let neha = t.as_person(1).await;
    t.post(&format!("/accept/{id}"), &neha, "").await;
    t.post(&format!("/accept/{id}"), &neha, "").await;
    assert_eq!(t.scalar(&format!("SELECT count(*) FROM history WHERE activity_id = {id} AND outcome = 'did_it'")).await, 1);
}

#[tokio::test]
async fn login_and_rate_limit() {
    let t = setup_with(false, &auth::hash("open sesame")).await;
    assert_eq!(t.get("/", "").await.location(), "/login");
    assert!(t.send(Method::POST, "/login", "", Some("passcode=nope"), false).await.body.contains("That&#x27;s not it"));
    let ok = t.send(Method::POST, "/login", "", Some("passcode=open+sesame"), false).await;
    assert_eq!((ok.status, ok.location()), (StatusCode::SEE_OTHER, "/who"));
    assert_eq!(t.get("/", &cookies(&ok.headers)).await.status, StatusCode::OK);
    for _ in 0..10 {
        t.send(Method::POST, "/login", "", Some("passcode=nope"), false).await;
    }
    let blocked = t.send(Method::POST, "/login", "", Some("passcode=open+sesame"), false).await;
    assert!(blocked.body.contains("Too many tries"), "even the right passcode waits out the lockout");
}

async fn match_ids(t: &T, cookie: &str, form: &str) -> Vec<i64> {
    let r = t.post("/match", cookie, form).await;
    let v: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    v["ids"].as_array().unwrap().iter().map(|i| i.as_i64().unwrap()).collect()
}

#[tokio::test]
async fn not_for_me_takes_a_slip_out_of_every_draw_until_brought_back() {
    let t = setup().await;
    let saltt = t.id_of("Dinner at Saltt").await;
    let drive = t.id_of("Long drive to Lonavala").await;
    let neha = t.as_person(1).await;
    assert!(match_ids(&t, &neha, "").await.contains(&drive));

    // Without saying who you are, a veto does nothing.
    t.post("/pick", "", &format!("veto=1&current={drive}&used=0")).await;
    assert_eq!(t.scalar("SELECT count(*) FROM vetoes").await, 0);

    let r = t.post("/pick", &neha, &format!("veto=1&current={drive}&used=1&excluded=")).await;
    assert_eq!(t.scalar(&format!("SELECT person FROM vetoes WHERE activity_id = {drive}")).await, 1);
    assert!(!r.body.contains(&format!("data-id=\"{drive}\"")));
    if !r.hidden("used").is_empty() {
        assert_eq!(r.hidden("used"), "1", "a veto doesn't cost a peek");
    }
    assert!(!match_ids(&t, &neha, "").await.contains(&drive), "/match skips it");
    for _ in 0..40 {
        assert!(!t.post("/pick", &neha, "").await.body.contains(&format!("data-id=\"{drive}\"")), "/pick skips it");
    }
    // Pair offers on Saltt no longer suggest the drive.
    t.exec("UPDATE activities SET archived = 1").await;
    t.exec(&format!("UPDATE activities SET archived = 0 WHERE id = {saltt}")).await;
    let card = t.post("/pick", &neha, "").await;
    assert!(card.body.contains(&format!("data-id=\"{saltt}\"")) && !card.body.contains("Long drive to Lonavala"));
    t.exec("UPDATE activities SET archived = 0").await;

    assert!(t.get("/wishlist", &neha).await.body.contains("Neha passed on this"));
    t.post(&format!("/wishlist/{drive}/unveto"), &neha, "").await;
    assert!(match_ids(&t, &neha, "").await.contains(&drive), "brought back");
}

#[tokio::test]
async fn closed_days_and_seasons_follow_today() {
    let t = setup().await;
    let planet = t.id_of("Planetarium show").await;
    let zoo = t.id_of("Zoo visit").await;
    // Closed every day / in season only next month: never today.
    t.exec(&format!("UPDATE activities SET closed_days = 'sun,mon,tue,wed,thu,fri,sat' WHERE id = {planet}")).await;
    t.exec(&format!(
        "UPDATE activities SET closed_days = '', season_from = (CAST(strftime('%m', 'now', 'localtime') AS INTEGER) % 12) + 1,
         season_to = (CAST(strftime('%m', 'now', 'localtime') AS INTEGER) % 12) + 1 WHERE id = {zoo}"
    ))
    .await;
    let ids = match_ids(&t, "", "").await;
    assert!(!ids.contains(&planet) && !ids.contains(&zoo));

    // Only the planetarium is left: the jar says why, with nothing to loosen.
    t.exec(&format!("UPDATE activities SET archived = 1 WHERE id != {planet}")).await;
    let r = t.post("/pick", "", "").await;
    assert!(r.body.contains("closed on"), "{}", r.body);
    assert!(!r.body.contains("f-relax"), "no relax button for a closed day");
    // The slip itself says so too.
    t.exec("UPDATE activities SET archived = 0").await;
    assert!(t.get("/wishlist", "").await.body.contains("Closed Sundays, Mondays, Tuesdays"));
}

#[tokio::test]
async fn every_slip_carries_its_search_text() {
    let t = setup().await;
    let body = t.get("/wishlist", "").await.body;
    assert!(body.contains("id=\"slip-q\""), "the search box is on the page");
    // Title, place, tags and notes, lowercased, so the box can match any of them.
    assert!(body.contains("data-search=\"ferry to elephanta caves "), "{body}");
    assert!(body.contains("closed mondays"), "notes are searchable too");
}

#[tokio::test]
async fn a_slip_can_be_planned_for_a_day_and_sent() {
    let t = setup().await;
    let id = t.id_of("Do a physical jigsaw puzzle together").await;
    let neha = t.as_person(1).await;
    let body = t.get("/wishlist", &neha).await.body;
    assert!(body.contains(&format!("data-plan=\"{id}\"")), "every slip has a Plan it button");

    let bad = t.send(Method::POST, &format!("/wishlist/{id}/plan"), &neha, Some("at="), true).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY, "a time is needed");
    assert_eq!(t.scalar(&format!("SELECT count(*) FROM history WHERE activity_id = {id}")).await, 0);

    let r = t.send(Method::POST, &format!("/wishlist/{id}/plan"), &neha, Some("at=2026-11-02T19%3A00"), true).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.body.contains("is planned for <strong>Mon 2 Nov, 7:00 pm</strong>"), "{}", r.body);
    assert!(r.body.contains("Send to Rohit"), "{}", r.body);
    assert!(r.body.contains("https://wa.me/?text=Our%20date%3A%20Do%20a%20physical%20jigsaw"), "{}", r.body);
    let hid = t.scalar(&format!("SELECT id FROM history WHERE activity_id = {id}")).await;
    assert_eq!(t.scalar(&format!("SELECT drawn_by FROM history WHERE id = {hid}")).await, 1);
    assert_eq!(
        t.scalar(&format!("SELECT planned_at = unixepoch('2026-11-02 19:00', 'utc') FROM history WHERE id = {hid}")).await,
        1
    );
    assert!(r.body.contains(&format!("/history/{hid}/invite.ics")));

    // Without htmx it lands on the kept date.
    let other = t.id_of("Cook together").await;
    let plain = t.send(Method::POST, &format!("/wishlist/{other}/plan"), &neha, Some("at=2026-11-03T20%3A00"), false).await;
    assert!(plain.location().starts_with("/history#k"), "{}", plain.location());
}

#[tokio::test]
async fn the_jar_learns_from_real_costs() {
    let t = setup().await;
    let cook = t.id_of("Cook together").await; // ₹800
    let book = t.id_of("Bookstore Roadies task").await; // ₹800
    // Three dates that really cost double.
    for a in [t.id_of("Cat café").await, t.id_of("Zoo visit").await, t.id_of("Arcade games").await] {
        t.exec(&format!(
            "INSERT INTO history (activity_id, outcome, est_cost, actual_cost)
             SELECT {a}, 'did_it', est_cost, est_cost * 2 FROM activities WHERE id = {a}"
        ))
        .await;
    }
    // This one's estimate already came from a real cost: left alone.
    t.exec(&format!("UPDATE activities SET est_from_actual = 1 WHERE id = {book}")).await;

    let ids = match_ids(&t, "", "budget=1000").await;
    assert!(!ids.contains(&cook), "₹800 × 2 = ₹1,600 no longer fits ₹1,000");
    assert!(ids.contains(&book), "a real-cost estimate isn't doubled");

    t.exec(&format!("UPDATE activities SET archived = 1 WHERE id != {cook}")).await;
    let card = t.post("/pick", "", "").await;
    assert!(card.body.contains("₹1,600") && card.body.contains("slip says ₹800"), "the card shows the same number");
    let kept = t.get("/history", "").await;
    assert!(kept.body.contains("about 100% over the estimate"), "Kept explains it");
}

#[tokio::test]
async fn the_budget_only_counts_what_we_really_spent() {
    let t = setup().await;
    let a = t.id_of("Cat café").await;
    let add = |actual: &str, planned: &str| {
        format!("INSERT INTO history (activity_id, outcome, est_cost, actual_cost, planned_at) VALUES ({a}, 'did_it', 900, {actual}, {planned})")
    };
    t.exec(&add("1500", "NULL")).await; // done this month, cost filled in: counts
    t.exec(&add("NULL", "NULL")).await; // kept but no real cost yet: doesn't
    t.exec(&add("700", "unixepoch() + 60 * 86400")).await; // happens in a later month
    t.exec(&add("400", "unixepoch() - 45 * 86400")).await; // happened in an earlier month
    assert_eq!(crate::db::spent_this_month(&t.db).await.unwrap(), 1500);
    assert!(t.get("/history", "").await.body.contains("<strong>₹1,500</strong>"));
}

#[tokio::test]
async fn spending_outside_a_slip_counts_toward_the_budget() {
    let t = setup().await;
    let r = t.post("/budget/spend", "", "amount=1%2C800&note=Dinner+at+Toto%27s&on=").await;
    assert!(r.status.is_redirection() || r.status.is_success());
    crate::db::add_other_spending(&t.db, 500, "", "2001-01-01").await.unwrap(); // an old month: doesn't count
    assert_eq!(crate::db::spent_this_month(&t.db).await.unwrap(), 1800);
    let id = crate::db::other_spending_this_month(&t.db).await.unwrap()[0].0;
    crate::db::delete_other_spending(&t.db, id).await.unwrap();
    assert_eq!(crate::db::spent_this_month(&t.db).await.unwrap(), 0);
}

#[tokio::test]
async fn last_month_s_leftover_can_be_rolled_over_or_dropped() {
    let t = setup().await;
    // The last day of last month, whatever today is ("40 days ago" is two months back early in a month).
    t.exec("INSERT INTO other_spending (amount, spent_on) VALUES (4200, date('now', 'localtime', 'start of month', '-1 day'))").await;
    // No budget set: nothing to offer.
    assert!(!t.get("/history", "").await.body.contains("/budget/rollover"));
    t.post("/budget", "", "amount=6000&back=%2Fhistory").await;
    let page = t.get("/history", "").await.body;
    assert!(page.contains("/budget/rollover") && page.contains("Roll ₹1,800 over"), "{page}");
    assert!(page.contains("of ₹6,000"));

    t.post("/budget/rollover", "", "choice=roll").await;
    let page = t.get("/history", "").await.body;
    assert!(!page.contains("/budget/rollover"), "decided: the card is gone");
    assert!(page.contains("of ₹7,800") && page.contains("includes ₹1,800 rolled over"), "{page}");
    assert!(page.contains("Past months") && page.contains("₹4,200"));

    t.post("/budget/rollover", "", "choice=fresh").await; // a second press changes nothing
    assert!(t.get("/history", "").await.body.contains("of ₹7,800"));
}

#[tokio::test]
async fn starting_fresh_or_overspending_carries_nothing() {
    let t = setup().await;
    t.post("/budget", "", "amount=6000&back=%2Fhistory").await;
    t.exec("INSERT INTO other_spending (amount, spent_on) VALUES (4200, date('now', 'localtime', 'start of month', '-1 day'))").await;
    t.post("/budget/rollover", "", "choice=fresh").await;
    let page = t.get("/history", "").await.body;
    assert!(page.contains("of ₹6,000") && !page.contains("rolled over"));

    let t = setup().await;
    t.post("/budget", "", "amount=3000&back=%2Fhistory").await;
    t.exec("INSERT INTO other_spending (amount, spent_on) VALUES (4200, date('now', 'localtime', 'start of month', '-1 day'))").await;
    assert!(!t.get("/history", "").await.body.contains("/budget/rollover"), "overspent: nothing to roll");
}

#[tokio::test]
async fn the_year_page_counts_spending_outside_a_slip_but_never_a_sealed_surprise() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    let year: i64 = t.scalar("SELECT CAST(strftime('%Y', 'now', 'localtime') AS INTEGER)").await;
    let a = t.id_of("Cat café").await;
    t.exec("INSERT INTO other_spending (amount, note, spent_on) VALUES (1800, 'Dinner', date('now', 'localtime', 'start of month'))").await;
    t.exec(&format!(
        "INSERT INTO history (activity_id, outcome, est_cost, actual_cost, planned_at, surprise, revealed, drawn_by)
         VALUES ({a}, 'did_it', 900, 5000, unixepoch() - 3600, 1, 0, 0)"
    ))
    .await;
    t.exec(&format!(
        "INSERT INTO history (activity_id, outcome, est_cost, actual_cost, planned_at, rating) VALUES ({a}, 'did_it', 900, 700, unixepoch() - 7200, 5)"
    ))
    .await;
    let page = t.get(&format!("/year/{year}"), &neha).await.body;
    assert!(page.contains("₹2,500"), "the dinner and the visible date add up: {page}");
    assert!(!page.contains("₹7,500") && !page.contains("₹5,000"), "the sealed surprise stays out");
    assert!(page.contains("y-blots") && page.contains("y-tally"));
}

#[tokio::test]
async fn a_month_spread_shows_its_dates_and_receipts_but_never_a_sealed_surprise() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    let cook = t.id_of("Cook together").await;
    let play = t.id_of(PLAY).await;
    t.exec(&format!(
        "INSERT INTO history (activity_id, outcome, est_cost, actual_cost, planned_at, rating, note)
         VALUES ({cook}, 'did_it', 900, 1300, unixepoch() - 3600, 5, 'Burnt the rice, worth it')"
    ))
    .await;
    t.exec(&format!(
        "INSERT INTO history (activity_id, outcome, est_cost, actual_cost, planned_at, surprise, revealed, drawn_by)
         VALUES ({play}, 'did_it', 900, 5000, unixepoch() - 3600, 1, 0, 0)"
    ))
    .await;
    t.exec("INSERT INTO other_spending (amount, note, spent_on) VALUES (1800, 'Dinner at Bastian', date('now', 'localtime', '-1 hour'))").await;
    let (y, m): (i64, i64) = sqlx::query_as(
        "SELECT CAST(strftime('%Y', unixepoch() - 3600, 'unixepoch', 'localtime') AS INTEGER),
                CAST(strftime('%m', unixepoch() - 3600, 'unixepoch', 'localtime') AS INTEGER)",
    )
    .fetch_one(&t.db)
    .await
    .unwrap();

    let page = t.get(&format!("/year/{y}/{m}"), &neha).await;
    assert_eq!(page.status, StatusCode::OK);
    let body = page.body;
    assert!(body.contains("Cook together") && body.contains("₹1,300") && body.contains("₹400 over our guess"), "{body}");
    assert!(body.contains("Burnt the rice, worth it") && body.contains("Dinner at Bastian"));
    assert!(!body.contains("Prithvi") && !body.contains("₹5,000"), "the sealed surprise stays out");

    // Both ways in link here, and nonsense months go back to the year.
    let href = format!("/year/{y}/{m}\"");
    assert!(t.get(&format!("/year/{y}"), &neha).await.body.contains(&href));
    assert!(t.get("/history", &neha).await.body.contains(&href));
    assert_eq!(t.get(&format!("/year/{y}/13"), &neha).await.status, StatusCode::SEE_OTHER);
    let empty = t.get(&format!("/year/{y}/{}", if m == 1 { 2 } else { m - 1 }), &neha).await;
    assert_eq!(empty.status, StatusCode::OK);
}

#[tokio::test]
async fn month_spreads_keep_sealed_surprises_out_of_totals_stickers_and_page_turns() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    let (cook, play) = (t.id_of("Cook together").await, t.id_of(PLAY).await);
    let at = |d: &str| format!("unixepoch('{d} 12:00:00')");
    // March and May are ours to see; April holds only a sealed surprise; December rolls into January.
    for (a, day, actual, rating, sealed) in [
        (cook, "2025-03-10", 1300, 3, 0),
        (play, "2025-03-20", 5000, 5, 1),
        (play, "2025-04-10", 2000, 5, 1),
        (cook, "2025-05-10", 700, 4, 0),
        (cook, "2025-12-10", 900, 4, 0),
        (cook, "2026-01-05", 600, 4, 0),
    ] {
        t.exec(&format!(
            "INSERT INTO history (activity_id, outcome, est_cost, actual_cost, rating, planned_at, picked_at, surprise, revealed, drawn_by)
             VALUES ({a}, 'did_it', 900, {actual}, {rating}, {}, {}, {sealed}, 0, 0)",
            at(day),
            at(day)
        ))
        .await;
    }

    let march = t.get("/year/2025/3", &neha).await.body;
    assert!(march.contains("₹1,300 spent"), "the sealed ₹5,000 isn't in the total: {march}");
    assert!(!march.contains("Prithvi") && !march.contains("₹5,000"));
    assert!(!march.contains("/year/2025/4\""), "a month holding only a sealed surprise isn't a page to turn to");
    assert!(march.contains("/year/2025/5\""));
    assert!(t.get("/year/2025/5", &neha).await.body.contains("/year/2025/3\""));
    let april = t.get("/year/2025/4", &neha).await.body;
    assert!(april.contains("nothing kept in April") && !april.contains("Prithvi"));

    // Turning the page across new year, both ways.
    assert!(t.get("/year/2025/12", &neha).await.body.contains("/year/2026/1\""));
    assert!(t.get("/year/2026/1", &neha).await.body.contains("/year/2025/12\""));

    // Kept links each month heading without zero padding.
    let kept = t.get("/history", &neha).await.body;
    assert!(kept.contains("/year/2025/3\"") && !kept.contains("/year/2025/03"));
}

#[tokio::test]
async fn nonsense_months_go_back_to_the_year() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    for (path, to) in [
        ("/year/2026/0", "/year/2026"),
        ("/year/2026/13", "/year/2026"),
        ("/year/2026/abc", "/year/2026"),
        ("/year/1999/5", "/year"),
        ("/year/2101/5", "/year"),
        ("/year/soon/5", "/year"),
    ] {
        let r = t.get(path, &neha).await;
        assert_eq!(r.status, StatusCode::SEE_OTHER, "{path}");
        assert_eq!(r.location(), to, "{path}");
    }
}

#[tokio::test]
async fn the_year_jar_holds_at_most_fourteen_slips_and_two_rows_of_stars() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    let cook = t.id_of("Cook together").await;
    let year: i64 = t.scalar("SELECT CAST(strftime('%Y', 'now', 'localtime') AS INTEGER)").await;
    // Twenty dates in the last day (all this year unless it's the small hours of 1 January).
    for n in 1..=20 {
        let rating = match n { 1 | 2 => "5", 3 => "4", 4 => "3", 5 => "1", _ => "NULL" };
        t.exec(&format!(
            "INSERT INTO history (activity_id, outcome, rating, picked_at) VALUES ({cook}, 'did_it', {rating}, unixepoch('now', '-{n} minutes'))"
        ))
        .await;
    }
    let page = t.get(&format!("/year/{year}"), &neha).await.body;
    assert_eq!(page.matches("class=\"y-jar-slip\"").count(), 14);
    assert_eq!(page.matches("class=\"y-gave-row\"").count(), 2, "only the two most-given ratings");
    assert!(page.contains("× 2") && page.contains("× 1"));
}

#[tokio::test]
async fn cab_estimates_calibrate_from_real_fares() {
    let t = setup().await;
    let sassoon = t.id_of("Sunrise at Sassoon Docks").await;
    let neha = t.as_person(1).await;
    let mut ids = vec![];
    for _ in 0..3 {
        t.exec(&format!("INSERT INTO history (activity_id, outcome, est_cost) VALUES ({sassoon}, 'did_it', 500)")).await;
        ids.push(t.scalar("SELECT max(id) FROM history").await);
    }
    // Neha's real fares are far above the model's (clamped when applied).
    for id in &ids {
        let r = t.post(&format!("/history/{id}/cost"), &neha, "actual=&cab_1=99999").await;
        assert_eq!(r.status, StatusCode::SEE_OTHER);
    }
    let (fare, est): (i64, i64) = sqlx::query_as(&format!("SELECT cab_fare_1, cab_est_1 FROM history WHERE id = {}", ids[0]))
        .fetch_one(&t.db)
        .await
        .unwrap();
    assert_eq!(fare, 99999);
    assert!(est > 0, "the model's estimate is saved next to the real fare");
    t.exec(&format!("UPDATE activities SET archived = 1 WHERE id != {sassoon}")).await;
    let card = t.post("/pick", &neha, "").await;
    assert!(card.body.contains("calibrated from 3 real rides"), "{}", card.body);
}

#[tokio::test]
async fn gentle_nudges() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    // A date a month ago, nothing planned: "It's been 4 weeks".
    let cook = t.id_of("Cook together").await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, picked_at) VALUES ({cook}, 'did_it', unixepoch('now', '-30 days'))")).await;
    assert!(t.get("/", &neha).await.body.contains("been 4 weeks since your last date"));

    // Rohit's surprise tomorrow evening: Neha sees who it's from, not what it is.
    let play = t.id_of(PLAY).await;
    t.exec(&format!(
        "INSERT INTO history (activity_id, outcome, drawn_by, surprise, planned_at)
         VALUES ({play}, 'did_it', 0, 1, unixepoch(date('now', 'localtime', '+1 day') || ' 19:00', 'utc'))"
    ))
    .await;
    let jar = without_slip_list(&t.get("/", &neha).await.body);
    assert!(jar.contains("Tomorrow, 7:00 pm: a surprise from Rohit"), "{jar}");
    assert!(!jar.contains("Prithvi") && !jar.contains("been 4 weeks"));
    let rohit = t.as_person(0).await;
    assert!(t.get("/", &rohit).await.body.contains("Tomorrow, 7:00 pm: A play at Prithvi Theatre"));
}

#[tokio::test]
async fn the_year_in_review_never_counts_a_sealed_surprise_or_names_us_in_the_picture() {
    let t = setup().await;
    let year: i64 = t.scalar("SELECT CAST(strftime('%Y', 'now', 'localtime') AS INTEGER)").await;
    let cook = t.id_of("Cook together").await;
    let hindi = sqlx::query_scalar::<_, i64>("SELECT id FROM activities WHERE title LIKE 'Hindi rap%'")
        .fetch_one(&t.db)
        .await
        .unwrap();
    for a in [cook, hindi] {
        t.exec(&format!("INSERT INTO history (activity_id, outcome, rating, picked_at) VALUES ({a}, 'did_it', 5, unixepoch('now', '-1 hour'))")).await;
    }
    surprise_from_rohit(&t).await; // planned in two days: still to come, and sealed
    let play = t.id_of(PLAY).await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, drawn_by, surprise, picked_at) VALUES ({play}, 'did_it', 0, 1, unixepoch('now', '-2 hours'))")).await;
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);

    for who in [&rohit, &neha] {
        let page = t.get(&format!("/year/{year}"), who).await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.body.contains("class=\"y-count\">2<"), "sealed surprises aren't counted, for anyone");
        assert!(!page.body.contains("Prithvi"));
    }
    let share = t.get(&format!("/year/{year}"), &neha).await.body;
    let island = share.split("id=\"year-share\">").nth(1).unwrap().split("</script>").next().unwrap();
    assert!(!island.contains("Neha") && !island.contains("Rohit"), "no names in the share data: {island}");
    assert!(island.contains("watch us react"), "names in titles become \"us\": {island}");
    assert_eq!(t.get("/year", &neha).await.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn offline_pieces_work_without_login_and_hold_nothing_personal() {
    let t = setup_with(false, &auth::hash("pw")).await;
    let sw = t.get("/sw.js", "").await;
    assert_eq!(sw.status, StatusCode::OK);
    assert!(sw.body.contains(&format!("const V = '{}'", crate::BUILD)));
    assert_eq!(sw.headers.get(header::CACHE_CONTROL).unwrap(), "no-cache");
    let off = t.get("/offline", "").await;
    assert_eq!(off.status, StatusCode::OK);
    assert!(!off.body.contains("Rohit") && !off.body.contains("Neha"));
    assert_eq!(t.get("/history", "").await.location(), "/login", "everything else still needs the passcode");
}

#[tokio::test]
async fn pages_are_never_left_in_the_browser_cache_and_carry_privacy_headers() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    let page = t.get("/history", &neha).await;
    assert_eq!(page.headers.get(header::CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(page.headers.get("x-joy-viewer").unwrap(), "1");
    assert_eq!(page.headers.get("referrer-policy").unwrap(), "no-referrer");
    assert_eq!(page.headers.get("x-content-type-options").unwrap(), "nosniff");
    assert_eq!(t.get("/history", "").await.headers.get("x-joy-viewer").unwrap(), "none");
    assert!(page.body.contains(&format!("/static/tokens.css?v={}", crate::BUILD)), "assets are versioned");
}

#[tokio::test]
async fn every_page_brings_its_motion_and_keeps_it_offline() {
    let t = setup().await;
    let neha = t.as_person(1).await;
    let script = format!("<script type=\"module\" src=\"/static/motion.js?v={}\">", crate::BUILD);
    let style = format!("/static/motion.css?v={}", crate::BUILD);
    for path in ["/", "/wishlist", "/history", "/who"] {
        let page = t.get(path, &neha).await;
        assert!(page.body.contains(&script) && page.body.contains(&style), "{path} loads motion");
    }
    // The page waits for motion.js before showing, but never under reduced motion.
    assert!(t.get("/", &neha).await.body.contains("matchMedia('(prefers-reduced-motion: reduce)').matches) document.documentElement.classList.add('m-in')"));
    let sw = t.get("/sw.js", "").await.body;
    assert!(sw.contains("'motion.css', 'motion.js'"), "motion is precached for offline");
}

#[tokio::test]
async fn logout_forgets_who_is_holding_the_phone_and_switching_clears_the_cache() {
    let t = setup().await;
    let switch = t.send(Method::POST, "/who", "", Some("who=1&pin=pin-of-1&pin2=pin-of-1"), false).await;
    assert!(switch.headers.get("clear-site-data").is_none(), "Clear-Site-Data: cache can stall the redirect");
    let out = t.send(Method::POST, "/logout", &cookies(&switch.headers), Some(""), false).await;
    assert_eq!(out.headers.get("clear-site-data").unwrap(), "\"storage\"");
    let cleared: Vec<String> = out.headers.get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap().to_string()).collect();
    assert!(cleared.iter().any(|c| c.starts_with("joy_who=") && c.contains("Max-Age=0")), "{cleared:?}");
}

#[tokio::test]
async fn a_year_whose_only_date_is_a_sealed_surprise_isnt_listed() {
    let t = setup().await;
    let play = t.id_of(PLAY).await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, drawn_by, surprise, picked_at) VALUES ({play}, 'did_it', 0, 1, unixepoch('2024-05-01'))")).await;
    let cook = t.id_of("Cook together").await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, picked_at) VALUES ({cook}, 'did_it', unixepoch('2025-05-01'))")).await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, picked_at) VALUES ({cook}, 'did_it', unixepoch('2026-01-10'))")).await;
    let neha = t.as_person(1).await;
    let page = t.get("/year/2025", &neha).await.body;
    let near: Vec<&str> = page.match_indices("/year/2024").map(|(i, _)| &page[i.saturating_sub(120)..(i + 40).min(page.len())]).collect();
    assert!(page.contains("/year/2026"), "the switcher lists the other visible year");
    assert!(near.is_empty(), "2024 holds only a sealed surprise: {near:?}");
}

#[tokio::test]
async fn meet_in_the_middle_draws_from_the_overlap_and_never_shows_the_other_s_answers() {
    let t = setup().await;
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);

    // Not both in yet: no middle draw, just a note.
    let early = t.post("/pick", &rohit, "middle=1").await;
    assert!(early.body.contains("Once you&#x27;ve both checked in") || early.body.contains("Once you've both checked in"), "{}", early.body);

    let r = t.post("/middle", &rohit, "energy=high&budget=3000&max_duration=full_day&reach=far&vibe=adventurous").await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let neha_jar = t.get("/", &neha).await.body;
    assert!(neha_jar.contains("Rohit has checked in for tonight"));
    for answer in [r#"value="high" checked"#, r#"value="3000" checked"#, r#"value="full_day" checked"#, r#"value="adventurous" checked"#] {
        assert!(!neha_jar.contains(answer), "Neha's page shows Rohit's answer {answer}");
    }

    t.post("/middle", &neha, "energy=low&budget=500&max_duration=short&reach=home&vibe=cozy").await;
    assert!(t.get("/", &rohit).await.body.contains("Draw from the middle"));

    let mut current = String::new();
    for i in 0..12 {
        let form = if i == 0 { "middle=1".to_string() } else { format!("middle=1&reroll=1&current={current}&used=0") };
        let card = t.post("/pick", &rohit, &form).await.body;
        assert!(card.contains("where you both meet tonight"), "{card}");
        assert!(!card.contains(r#"name="budget""#) && !card.contains(r#"name="energy""#), "the card carries the merged answers");
        assert!(!card.contains("Stretch to"), "a relax hint gives an answer away");
        let Some(id) = card.split(r#"data-id=""#).nth(1).and_then(|r| r.split('"').next()) else { break };
        current = id.to_string();
        let (setting, energy, cost): (String, String, i64) =
            sqlx::query_as(&format!("SELECT setting, energy, est_cost FROM activities WHERE id = {id}")).fetch_one(&t.db).await.unwrap();
        assert_eq!(setting, "home", "Neha wants to stay in");
        assert_eq!(energy, "low", "Neha is tired");
        assert!(cost <= 500, "Neha's budget");
    }

    assert!(!current.is_empty(), "the middle draw found something cozy at home");

    // Taking a check-in back ends the middle draw.
    t.post("/middle", &neha, "energy=low&clear=1").await;
    assert!(!t.get("/", &rohit).await.body.contains("Draw from the middle"));
    assert_eq!(t.scalar("SELECT count(*) FROM checkins").await, 1);
}

#[tokio::test]
async fn switching_to_the_other_person_takes_their_own_passcode() {
    let t = setup().await;
    surprise_from_rohit(&t).await;
    let rohit = t.as_person(0).await;
    let neha = t.as_person(1).await;

    // Neha tries to become Rohit on her phone: wrong passcode, no switch, still sealed.
    let tries = t.send(Method::POST, "/who", &neha, Some("who=0&pin=guess"), false).await;
    assert_eq!(tries.status, StatusCode::OK);
    assert!(tries.body.contains("That&#x27;s not it") || tries.body.contains("That's not it"), "{}", tries.body);
    assert!(!cookies(&tries.headers).contains("joy_who"), "a wrong passcode doesn't switch");
    assert!(!t.get("/history", &neha).await.body.contains("Prithvi"));
    // Nor can she set one for him: his already exists, so "pin2" is ignored.
    let claim = t.send(Method::POST, "/who", &neha, Some("who=0&pin=mine&pin2=mine"), false).await;
    assert!(!cookies(&claim.headers).contains("joy_who"));

    // First time, the two passcodes have to match and be long enough.
    let t2 = setup().await;
    let short = t2.send(Method::POST, "/who", "", Some("who=1&pin=12&pin2=12"), false).await;
    assert!(short.body.contains("at least 4"));
    let typo = t2.send(Method::POST, "/who", "", Some("who=1&pin=1234&pin2=1243"), false).await;
    assert!(typo.body.contains("match"));

    // Rohit changes his passcode on another phone: the old cookie stops being Rohit.
    assert!(t.get("/history", &rohit).await.body.contains("Prithvi"));
    let changed = t.send(Method::POST, "/who", "", Some("who=0&pin=pin-of-0&new_pin=fresh-one&new_pin2=fresh-one"), false).await;
    assert_eq!(changed.status, StatusCode::SEE_OTHER, "{}", changed.body);
    let stale = t.get("/history", &rohit).await;
    assert!(!stale.body.contains("Prithvi"), "a cookie from before the change still sees the surprise");
    let gone: Vec<String> = stale.headers.get_all(header::SET_COOKIE).iter().map(|v| v.to_str().unwrap().to_string()).collect();
    assert!(gone.iter().any(|c| c.starts_with("joy_who=") && c.contains("Max-Age=0")), "{gone:?}");
    assert!(t.get("/history", &cookies(&changed.headers)).await.body.contains("Prithvi"));
}

#[tokio::test]
async fn together_and_apart_are_two_kinds_of_home() {
    let t = setup().await;
    let apart: i64 = t.scalar("SELECT count(*) FROM activities WHERE setting = 'apart'").await;
    assert!(apart >= 60, "the long-distance slips moved to their own setting");
    assert_eq!(t.scalar("SELECT count(*) FROM activities WHERE tags LIKE '%long-distance%'").await, 0);
    let setting_of = |card: &str| card.split(r#"data-setting=""#).nth(1).and_then(|r| r.split('"').next()).map(String::from);

    for _ in 0..8 {
        let card = t.post("/pick", "", "settings=apart").await.body;
        assert_eq!(setting_of(&card).as_deref(), Some("apart"), "{card}");
    }
    for _ in 0..8 {
        let card = t.post("/pick", "", "settings=home").await.body;
        assert_eq!(setting_of(&card).as_deref(), Some("home"));
    }

    // Meet in the middle: one of us is away, so only long-distance slips; together, never them.
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);
    t.post("/middle", &rohit, "energy=high&reach=far").await;
    t.post("/middle", &neha, "energy=high&reach=apart").await;
    for _ in 0..8 {
        let card = t.post("/pick", &rohit, "middle=1").await.body;
        assert_eq!(setting_of(&card).as_deref(), Some("apart"), "{card}");
    }
    t.post("/middle", &neha, "energy=high&reach=far").await;
    for _ in 0..12 {
        let card = t.post("/pick", &rohit, "middle=1").await.body;
        assert_ne!(setting_of(&card).as_deref(), Some("apart"), "together tonight, but drew a long-distance slip");
    }
    assert!(t.get("/", &rohit).await.body.contains("long distance"), "the jar sentence offers it");
}

#[tokio::test]
async fn each_line_stays_sealed_until_both_of_us_have_written_one() {
    let t = setup().await;
    let a = t.id_of("Cat café").await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, est_cost) VALUES ({a}, 'did_it', 900)")).await;
    let h = t.scalar("SELECT max(id) FROM history").await;
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);

    t.post(&format!("/history/{h}/line"), &rohit, "line=The+grey+one+chose+you").await;
    let page = t.get("/history", &neha).await.body;
    assert!(!page.contains("grey one"), "Neha reads Rohit's line before writing hers");
    assert!(page.contains("Rohit wrote a line. Write yours to read it."));
    assert!(!t.get("/history", "").await.body.contains("grey one"), "an unknown device reads it");
    assert!(t.get("/history", &rohit).await.body.contains("Sealed until Neha writes one too."));

    t.post(&format!("/history/{h}/line"), &neha, "line=Seven+cats+on+one+lap").await;
    let (page_r, page_n) = (t.get("/history", &rohit).await.body, t.get("/history", &neha).await.body);
    assert!(page_r.contains("Seven cats") && page_r.contains("grey one"));
    assert!(page_n.contains("Seven cats") && page_n.contains("grey one"));

    // Taking yours back seals theirs again; nobody can write without saying who they are.
    t.post(&format!("/history/{h}/line"), &neha, "line=+").await;
    assert!(!t.get("/history", &neha).await.body.contains("grey one"));
    assert_eq!(t.post(&format!("/history/{h}/line"), "", "line=hi").await.status, StatusCode::FORBIDDEN);

    t.post(&format!("/history/{h}/delete"), &rohit, "").await;
    assert_eq!(t.scalar("SELECT count(*) FROM memories").await, 0, "lines go with the date");
}

#[tokio::test]
async fn a_line_can_t_be_written_on_a_surprise_still_sealed_for_you() {
    let t = setup().await;
    let h = surprise_from_rohit(&t).await;
    let neha = t.as_person(1).await;
    assert_eq!(t.post(&format!("/history/{h}/line"), &neha, "line=hm").await.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn the_savings_jar_fills_from_leftovers_and_pays_for_its_date() {
    let t = setup().await;
    let goal = t.id_of("Cat café").await;
    t.post("/budget", "", "amount=6000&back=%2Fhistory").await;
    t.exec("INSERT INTO other_spending (amount, spent_on) VALUES (5000, date('now', 'localtime', 'start of month', '-1 day'))").await;
    assert!(!t.get("/history", "").await.body.contains("value=\"save\""), "no goal: no savings choice");

    t.post("/savings/goal", "", &format!("id={goal}")).await;
    let page = t.get("/history", "").await.body;
    assert!(page.contains("value=\"save\"") && page.contains("Saving for <strong>Cat café</strong>: ₹0 of ₹900"), "{page}");

    t.post("/budget/rollover", "", "choice=save").await;
    t.post("/budget/rollover", "", "choice=save").await; // a second press adds nothing
    assert_eq!(crate::db::savings_balance(&t.db).await.unwrap(), 1000);
    let page = t.get("/history", "").await.body;
    assert!(page.contains("of ₹6,000") && !page.contains("rolled over"), "saving it doesn't roll it over");
    assert!(page.contains("/savings/keep"), "₹1,000 covers ₹900");

    let rohit = t.as_person(0).await;
    let r = t.post("/savings/keep", &rohit, "").await;
    let h = t.scalar("SELECT max(id) FROM history").await;
    assert_eq!(r.location(), format!("/history#k{h}"));
    t.post("/savings/keep", &rohit, "").await; // double tap
    assert_eq!(t.scalar("SELECT count(*) FROM history WHERE outcome = 'did_it'").await, 1);
    assert_eq!(crate::db::savings_balance(&t.db).await.unwrap(), 100);
    assert!(crate::db::setting(&t.db, "saving_for").await.unwrap().is_none(), "the goal is done");

    // What the jar paid doesn't count against this month; anything over it does.
    t.post(&format!("/history/{h}/cost"), &rohit, "actual=1200").await;
    assert_eq!(crate::db::spent_this_month(&t.db).await.unwrap(), 300);

    // Deleting the date puts the money back.
    t.post(&format!("/history/{h}/delete"), &rohit, "").await;
    assert_eq!(crate::db::savings_balance(&t.db).await.unwrap(), 1000);
}

#[tokio::test]
async fn money_put_in_by_hand_can_be_taken_back_and_isn_t_spending() {
    let t = setup().await;
    t.post("/savings/add", "", "amount=2%2C500&note=Diwali+bonus").await;
    assert_eq!(crate::db::savings_balance(&t.db).await.unwrap(), 2500);
    assert_eq!(crate::db::spent_this_month(&t.db).await.unwrap(), 0);
    let goal = t.id_of("Cat café").await;
    t.post("/savings/goal", "", &format!("id={goal}")).await;
    t.post("/savings/goal", "", "id=").await; // stop saving for it: the money stays
    assert_eq!(crate::db::savings_balance(&t.db).await.unwrap(), 2500);
    assert_eq!(t.post("/savings/keep", "", "").await.location(), "/history#k-savings");
    let id = t.scalar("SELECT max(id) FROM savings").await;
    t.post(&format!("/savings/{id}/delete"), "", "").await;
    assert_eq!(crate::db::savings_balance(&t.db).await.unwrap(), 0);
}

// ── the day-of plan ───────────────────────────────────────────────────────

/// A jar that knows when we meet: Tue/Thu after work from the office, Sat/Sun from home.
async fn setup_scheduled() -> T {
    setup_custom(true, "unused", |c| {
        let vars = [
            ("DATE_DAYS", "tue,thu,sat,sun"),
            ("OFFICE", "19.1741, 72.8580"),
            ("OFFICE_NAME", "the office"),
            ("LATEST_A", "22:00"),
            ("LATEST_A_AT", "19.1646, 72.8493"),
            ("LATEST_A_PLACE", "Goregaon station"),
            ("LATEST_B", "23:00"),
        ];
        c.schedule = crate::schedule::Schedule::from_vars(|k| vars.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string()));
    })
    .await
}

/// The jar page's date choices: (value, label).
fn whens(jar: &str) -> Vec<(String, String)> {
    let Some(sel) = jar.split("name=\"when\"").nth(1).and_then(|r| r.split("</select>").next()) else { return vec![] };
    sel.split("<option value=\"")
        .skip(1)
        .filter_map(|o| {
            let (v, rest) = o.split_once('"')?;
            let label = rest.split_once('>')?.1.split('<').next()?;
            Some((v.to_string(), label.to_string()))
        })
        .collect()
}

/// A Versova café near the office: books ahead, needs cash, open late.
async fn near_cafe(t: &T) -> i64 {
    t.exec(
        "INSERT INTO activities (title, est_cost, setting, venue, area, lat, lng, duration, energy, hours, minutes, booking,
           book_ahead_days, cover, bring, station)
         VALUES ('Coffee by the creek', 800, 'local', 'Creek Café', 'Versova', 19.135, 72.815, 'short', 'low',
           'mon-sun 08:00-23:30', 90, 'always', 1, 'indoor', 'cash, a book', 'Versova')",
    )
    .await;
    t.id_of("Coffee by the creek").await
}

#[tokio::test]
async fn the_jar_draws_for_our_next_date_and_only_what_fits_it() {
    let t = setup_scheduled().await;
    let neha = t.as_person(1).await;
    let jar = t.get("/", &neha).await.body;
    let w = whens(&jar);
    assert!(w.len() >= 3, "a few dates to choose from: {w:?}");
    assert!(jar.contains("<option value=\"any\">any day at all</option>"));
    let weeknight = w.iter().find(|(_, l)| l.ends_with("evening")).expect("a Tue or Thu evening").0.clone();

    let near = near_cafe(&t).await;
    t.exec("INSERT INTO activities (title, est_cost, setting, venue, lat, lng, duration, energy, minutes)
            VALUES ('Long lunch in Colaba', 900, 'local', 'Colaba Causeway', 18.92, 72.83, 'short', 'low', 120)").await;
    let far = t.id_of("Long lunch in Colaba").await;

    let ids = match_ids(&t, &neha, &format!("when={weeknight}")).await;
    assert!(ids.contains(&near), "a café near the office fits a weeknight");
    assert!(!ids.contains(&far), "Colaba is too far after work");
    let any = match_ids(&t, &neha, "when=any").await;
    assert!(any.contains(&near) && any.contains(&far), "any day lifts the window");

    // Nothing else fits: the jar says why, and offers to draw for any day.
    t.exec(&format!("UPDATE activities SET archived = 1 WHERE id != {far}")).await;
    let r = t.post("/pick", &neha, &format!("when={weeknight}")).await;
    assert!(r.body.contains("Colaba") && r.body.contains("weeknight"), "{}", r.body);
    assert!(r.body.contains("Draw for any day"));
    assert!(r.body.contains("name=\"when\" value=\"any\""));
}

#[tokio::test]
async fn keeping_a_date_plans_the_day_deals_the_jobs_and_folds_in_a_plan_b() {
    let t = setup_scheduled().await;
    let neha = t.as_person(1).await;
    let w = whens(&t.get("/", &neha).await.body);
    let weeknight = w.iter().find(|(_, l)| l.ends_with("evening")).unwrap().0.clone();
    let cafe = near_cafe(&t).await;
    // An outdoor walk next door, with the café as the obvious Plan B.
    t.exec("INSERT INTO activities (title, est_cost, setting, venue, lat, lng, duration, energy, minutes, cover)
            VALUES ('Sunset on the jetty', 0, 'local', 'Versova jetty', 19.136, 72.814, 'short', 'low', 60, 'outdoor')").await;
    let walk = t.id_of("Sunset on the jetty").await;

    // The drawn card says when, before we commit.
    let card = t.post("/pick", &neha, &format!("when={weeknight}&current={walk}&excluded=")).await;
    assert!(card.body.contains("leave 7:30 pm") || card.body.contains("plan-line"), "{}", card.body);

    let r = t.post(&format!("/accept/{cafe}?when={weeknight}"), &neha, "").await;
    assert_eq!(r.status, StatusCode::OK);
    let hid = t.scalar(&format!("SELECT id FROM history WHERE activity_id = {cafe}")).await;
    assert_eq!(t.scalar(&format!("SELECT planned_at IS NOT NULL FROM history WHERE id = {hid}")).await, 1, "the time is set for us");
    assert_eq!(
        t.scalar(&format!("SELECT strftime('%H:%M', planned_at, 'unixepoch', 'localtime') > '19:30' FROM history WHERE id = {hid}")).await,
        1
    );
    assert!(r.body.contains("Leave the office together"), "{}", r.body);
    assert!(r.body.contains("Rohit at Goregaon station"));
    assert!(r.body.contains("Book Creek Café") && r.body.contains("Bring cash") && r.body.contains("Bring a book"));
    // Neha drew it, so Rohit gets the first job; they alternate.
    assert_eq!(t.scalar(&format!("SELECT who FROM tasks WHERE history_id = {hid} ORDER BY id LIMIT 1")).await, 0);
    assert_eq!(t.scalar(&format!("SELECT count(DISTINCT who) FROM tasks WHERE history_id = {hid}")).await, 2);
    // A double tap deals nothing twice.
    t.post(&format!("/accept/{cafe}?when={weeknight}"), &neha, "").await;
    assert_eq!(t.scalar(&format!("SELECT count(*) FROM tasks WHERE history_id = {hid}")).await, 3);

    // The walk's Plan B is the café next door; swap and swap back.
    let r = t.post(&format!("/accept/{walk}?when={weeknight}"), &neha, "").await;
    let walk_h = t.scalar(&format!("SELECT id FROM history WHERE activity_id = {walk}")).await;
    assert_eq!(t.scalar(&format!("SELECT plan_b FROM history WHERE id = {walk_h}")).await, cafe);
    assert!(r.body.contains("plan B, folded in"));
    let swapped = t.post(&format!("/history/{walk_h}/plan-b"), &neha, "").await;
    assert!(swapped.body.contains("Coffee by the creek"), "{}", swapped.body);
    assert_eq!(t.scalar(&format!("SELECT activity_id FROM history WHERE id = {walk_h}")).await, cafe);
    t.post(&format!("/history/{walk_h}/plan-b"), &neha, "").await;
    assert_eq!(t.scalar(&format!("SELECT activity_id FROM history WHERE id = {walk_h}")).await, walk);

    // The invite carries the plan, and its reminder is for leaving.
    let ics = t.get(&format!("/history/{hid}/invite.ics"), &neha).await.body;
    assert!(ics.contains("Leave the office together") && ics.contains("Time to get ready to leave"), "{ics}");
}

#[tokio::test]
async fn only_whoever_was_dealt_a_job_ticks_it_off() {
    let t = setup_scheduled().await;
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);
    let cafe = near_cafe(&t).await;
    t.exec(&format!("INSERT INTO history (activity_id, outcome, drawn_by, planned_at) VALUES ({cafe}, 'did_it', 1, unixepoch('now', '+1 day'))")).await;
    let hid = t.scalar("SELECT max(id) FROM history").await;
    t.exec(&format!("INSERT INTO tasks (history_id, who, task) VALUES ({hid}, 0, 'Book Creek Café')")).await;
    let task = t.scalar("SELECT max(id) FROM tasks").await;

    assert_eq!(t.post(&format!("/tasks/{task}/done"), &neha, "done=1").await.status, StatusCode::FORBIDDEN);
    assert_eq!(t.post(&format!("/tasks/{task}/done"), "", "done=1").await.status, StatusCode::FORBIDDEN, "nobody in particular");
    let r = t.post(&format!("/tasks/{task}/done"), &rohit, "done=1").await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.body.contains("is-done"));
    assert_eq!(t.scalar(&format!("SELECT done_at IS NOT NULL FROM tasks WHERE id = {task}")).await, 1);
    t.post(&format!("/tasks/{task}/done"), &rohit, "done=0").await;
    assert_eq!(t.scalar(&format!("SELECT done_at IS NULL FROM tasks WHERE id = {task}")).await, 1);

    // The jar page shows the next date's plan, with the jobs.
    let jar = t.get("/", &rohit).await.body;
    assert!(jar.contains("Your next date") && jar.contains("Book Creek Café"), "{jar}");
}

#[tokio::test]
async fn a_surprise_s_plan_gives_the_partner_only_hints() {
    let t = setup_scheduled().await;
    let (rohit, neha) = (t.as_person(0).await, t.as_person(1).await);
    let cafe = near_cafe(&t).await;
    t.exec(&format!(
        "INSERT INTO history (activity_id, outcome, drawn_by, surprise, planned_at, plan_b) VALUES ({cafe}, 'did_it', 0, 1, unixepoch('now', '+1 day'), {cafe})"
    ))
    .await;
    let hid = t.scalar("SELECT max(id) FROM history").await;
    t.exec(&format!("INSERT INTO tasks (history_id, who, task) VALUES ({hid}, 0, 'Book Creek Café')")).await;

    let jar = without_slip_list(&t.get("/", &neha).await.body);
    assert!(jar.contains("A surprise"), "{jar}");
    for leak in ["Creek", "Versova", "Book "] {
        assert!(!jar.contains(leak), "the surprise's plan leaks {leak}");
    }
    assert_eq!(t.post(&format!("/history/{hid}/plan-b"), &neha, "").await.status, StatusCode::FORBIDDEN);
    assert!(t.get("/", &rohit).await.body.contains("Book Creek Café"), "the planner sees their jobs");
}

#[tokio::test]
async fn slip_details_are_saved_and_bad_hours_are_caught() {
    let t = setup().await;
    let base = "title=Jazz+at+the+corner&est_cost=1200&setting=local&duration=short&energy=med&location=19.06%2C72.83";
    let bad = t.post("/wishlist", "", &format!("{base}&hours=evenings")).await;
    assert!(bad.body.contains("Hours should look like"), "{}", bad.body);
    assert_eq!(t.scalar("SELECT count(*) FROM activities WHERE title = 'Jazz at the corner'").await, 0);

    let r = t
        .post("/wishlist", "", &format!("{base}&hours=Tue-Sun+19%3A00-01%3A00&best_time=night&minutes=120&booking=always&book_ahead_days=3&cover=indoor&bring=cash%2C+++ID&station=Khar&station_walk_min=8"))
        .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let id = t.id_of("Jazz at the corner").await;
    let row: (String, String, i64, String, String, String) = sqlx::query_as(
        "SELECT hours, best_time, minutes, booking, bring, station FROM activities WHERE id = ?",
    )
    .bind(id)
    .fetch_one(&t.db)
    .await
    .unwrap();
    assert_eq!(row, ("tue-sun 19:00-01:00".into(), "night".into(), 120, "always".into(), "cash, ID".into(), "Khar".into()));

    // Only known values get in.
    t.post(&format!("/wishlist/{id}"), "", &format!("{base}&best_time=whenever&booking=maybe&cover=sky&minutes=-5")).await;
    let row: (String, String, String, Option<i64>) =
        sqlx::query_as("SELECT best_time, booking, cover, minutes FROM activities WHERE id = ?").bind(id).fetch_one(&t.db).await.unwrap();
    assert_eq!(row, (String::new(), String::new(), String::new(), None));
}

#[tokio::test]
async fn a_stale_date_on_an_old_card_plans_nothing() {
    let t = setup_scheduled().await;
    let neha = t.as_person(1).await;
    let cafe = near_cafe(&t).await;
    let r = t.post(&format!("/accept/{cafe}?when=2000-01-04"), &neha, "").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(t.scalar(&format!("SELECT planned_at IS NULL FROM history WHERE activity_id = {cafe}")).await, 1);
    assert!(r.body.contains("When is it?"), "asks for a time instead");
}
