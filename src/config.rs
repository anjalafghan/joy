use anyhow::{bail, Context, Result};
use axum_extra::extract::cookie::Key;

pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub passcode_hash: String,
    pub key: Key,
    pub cookie_secure: bool,
    pub currency: String,
    /// Skips the passcode. Only for local design work; never set this on a real deploy.
    pub dev_no_login: bool,
    /// The two of us: (name, home lat/lng). Homes only live in .env, never in the DB.
    pub people: [(String, Option<(f64, f64)>); 2],
    /// Neighbourhood names (e.g. "Bandra") used in personal slip notes. Optional.
    pub areas: [Option<String>; 2],
}

impl Config {
    pub fn from_env() -> Result<Self> {
        // dotenvy strips quotes but `docker run --env-file` doesn't, so strip one matching pair here.
        let var = |k: &str| {
            let v = std::env::var(k).ok()?;
            let v = v.trim();
            let v = ['\'', '"']
                .iter()
                .find_map(|q| v.strip_prefix(*q).and_then(|s| s.strip_suffix(*q)))
                .unwrap_or(v);
            (!v.trim().is_empty()).then(|| v.to_string())
        };

        let Some(passcode_hash) = var("APP_PASSCODE_HASH") else {
            bail!("APP_PASSCODE_HASH is not set. Run `cargo run -- hash-passcode` and put the output in .env");
        };
        let key = match var("SESSION_SECRET") {
            Some(s) if s.len() >= 64 => Key::from(s.as_bytes()),
            Some(_) => bail!("SESSION_SECRET must be at least 64 characters"),
            None => {
                tracing::warn!("SESSION_SECRET not set; logins will reset whenever the server restarts");
                Key::generate()
            }
        };

        Ok(Self {
            database_url: var("DATABASE_URL").unwrap_or_else(|| "sqlite://data/joy.db?mode=rwc".into()),
            port: var("PORT").map(|p| p.parse()).transpose().context("PORT")?.unwrap_or(3000),
            passcode_hash,
            key,
            cookie_secure: var("COOKIE_SECURE").is_some_and(|v| v == "true" || v == "1"),
            currency: var("CURRENCY_SYMBOL").unwrap_or_else(|| "₹".into()),
            dev_no_login: var("JOY_DEV_NO_LOGIN").is_some_and(|v| v == "1"),
            people: [
                (var("PERSON_A").unwrap_or_else(|| "Rohit".into()), var("HOME_A").and_then(|s| parse_latlng(&s))),
                (var("PERSON_B").unwrap_or_else(|| "Neha".into()), var("HOME_B").and_then(|s| parse_latlng(&s))),
            ],
            areas: [var("HOME_A_AREA"), var("HOME_B_AREA")],
        })
    }

    pub fn money(&self, amount: i64) -> String {
        if amount == 0 {
            return "Free".into();
        }
        let digits = amount.to_string();
        let grouped = if self.currency == "₹" { group_indian(&digits) } else { group_western(&digits) };
        format!("{}{}", self.currency, grouped)
    }

    /// Fills `{a}`, `{b}`, `{a_area}`, `{b_area}` in slip text from .env, so the shared
    /// seed stays generic. A `[[…]]` block only shows when every value inside it is set.
    pub fn personalize(&self, text: &str) -> String {
        if !text.contains('{') && !text.contains("[[") {
            return text.to_string();
        }
        let vars: [(&str, Option<&str>); 4] = [
            ("{a_area}", self.areas[0].as_deref()),
            ("{b_area}", self.areas[1].as_deref()),
            ("{a}", Some(&self.people[0].0)),
            ("{b}", Some(&self.people[1].0)),
        ];
        let fill = |s: &str| -> Option<String> {
            let mut out = s.to_string();
            for (key, val) in &vars {
                if out.contains(key) {
                    out = out.replace(key, (*val)?);
                }
            }
            Some(out)
        };
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find("[[") {
            out += &fill(&rest[..start]).unwrap_or_default();
            let after = &rest[start + 2..];
            let Some(end) = after.find("]]") else {
                rest = after;
                break;
            };
            out += &fill(&after[..end]).unwrap_or_default();
            rest = &after[end + 2..];
        }
        out += &fill(rest).unwrap_or_default();
        out
    }

    /// Approx road km from each person's home to a venue.
    pub fn distances(&self, venue: Option<(f64, f64)>) -> [Option<f64>; 2] {
        let d = |home: Option<(f64, f64)>| Some(road_km(home?, venue?));
        [d(self.people[0].1), d(self.people[1].1)]
    }
}

/// Accepts "19.13, 72.81" (what Google Maps copies on long-press).
pub fn parse_latlng(s: &str) -> Option<(f64, f64)> {
    let (a, b) = s.split_once(',')?;
    let (lat, lng) = (a.trim().parse::<f64>().ok()?, b.trim().parse::<f64>().ok()?);
    ((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lng)).then_some((lat, lng))
}

/// Straight-line distance × 1.3, a common rule of thumb for city road distance.
pub fn road_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (a.0.to_radians(), a.1.to_radians(), b.0.to_radians(), b.1.to_radians());
    let h = ((la2 - la1) / 2.0).sin().powi(2) + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    6371.0 * 2.0 * h.sqrt().asin() * 1.3
}

fn group_western(d: &str) -> String {
    let mut out = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn group_indian(d: &str) -> String {
    if d.len() <= 3 {
        return d.into();
    }
    let (head, tail) = d.split_at(d.len() - 3);
    let mut out = String::new();
    for (i, c) in head.chars().enumerate() {
        if i > 0 && (head.len() - i) % 2 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    format!("{out},{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_money() {
        assert_eq!(group_indian("150000"), "1,50,000");
        assert_eq!(group_indian("1200"), "1,200");
        assert_eq!(group_indian("800"), "800");
        assert_eq!(group_western("1500000"), "1,500,000");
    }

    fn cfg(areas: [Option<&str>; 2]) -> Config {
        Config {
            database_url: String::new(),
            port: 0,
            passcode_hash: String::new(),
            key: Key::generate(),
            cookie_secure: false,
            currency: "₹".into(),
            dev_no_login: false,
            people: [("Rohit".into(), None), ("Neha".into(), None)],
            areas: areas.map(|a| a.map(String::from)),
        }
    }

    #[test]
    fn personalizes_slip_text() {
        let mine = cfg([Some("Powai"), Some("Bandra")]);
        let generic = cfg([None, None]);
        let note = "Go early.[[ Close to {b_area}.]]";
        assert_eq!(mine.personalize(note), "Go early. Close to Bandra.");
        assert_eq!(generic.personalize(note), "Go early.");
        assert_eq!(generic.personalize("Hindi rap (watch {b} react)"), "Hindi rap (watch Neha react)");
        assert_eq!(
            mine.personalize("[[Midway between {a_area} and {b_area}. ]]Pin unverified."),
            "Midway between Powai and Bandra. Pin unverified."
        );
        assert_eq!(generic.personalize("plain text"), "plain text");
    }

    #[test]
    fn parses_latlng_and_measures() {
        assert_eq!(parse_latlng("19.13, 72.81"), Some((19.13, 72.81)));
        assert_eq!(parse_latlng("nope"), None);
        // Gateway of India -> Bandra Bandstand is ~13 km straight line
        let km = road_km((18.922, 72.834), (19.045, 72.818)) / 1.3;
        assert!((13.0..15.0).contains(&km), "{km}");
    }
}
