//! Tonight's rain chance from Open-Meteo (free, no key). Cached for an hour and
//! fails open: any error just means "no forecast", never a broken jar.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Deserialize;

/// Rain counts as "likely" at or above this chance.
pub const LIKELY_PERCENT: u8 = 60;
const FRESH: Duration = Duration::from_secs(60 * 60);
const RETRY_AFTER_ERROR: Duration = Duration::from_secs(10 * 60);

pub struct Weather {
    /// Where to look: between our two homes. None turns the feature off.
    at: Option<(f64, f64)>,
    client: reqwest::Client,
    cache: Mutex<Option<(Instant, Option<u8>)>>,
    /// WEATHER_FAKE_PERCENT, for design work: pretend tonight's rain chance is this.
    fake: Option<u8>,
}

#[derive(Deserialize)]
struct Forecast {
    hourly: Hourly,
}

#[derive(Deserialize)]
struct Hourly {
    time: Vec<String>,
    precipitation_probability: Vec<Option<u8>>,
}

impl Weather {
    pub fn new(at: Option<(f64, f64)>, fake: Option<u8>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .expect("http client");
        Self { at, client, cache: Mutex::new(None), fake }
    }

    /// Highest rain chance between 17:00 and 23:00 local time today, if we know it.
    pub async fn tonight(&self) -> Option<u8> {
        if self.fake.is_some() {
            return self.fake;
        }
        let (lat, lng) = self.at?;
        if let Some((at, value)) = *self.cache.lock().unwrap() {
            let ttl = if value.is_some() { FRESH } else { RETRY_AFTER_ERROR };
            if at.elapsed() < ttl {
                return value;
            }
        }
        let value = self.fetch(lat, lng).await;
        if value.is_none() {
            tracing::warn!("weather: no forecast, carrying on without it");
        }
        *self.cache.lock().unwrap() = Some((Instant::now(), value));
        value
    }

    async fn fetch(&self, lat: f64, lng: f64) -> Option<u8> {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={lat:.3}&longitude={lng:.3}\
             &hourly=precipitation_probability&forecast_days=1&timezone=auto"
        );
        let f: Forecast = self.client.get(url).send().await.ok()?.error_for_status().ok()?.json().await.ok()?;
        evening_max(&f.hourly)
    }
}

/// Times look like "2026-10-17T19:00" in the location's own timezone.
fn evening_max(h: &Hourly) -> Option<u8> {
    h.time
        .iter()
        .zip(&h.precipitation_probability)
        .filter(|(t, _)| t.get(11..13).and_then(|hh| hh.parse::<u8>().ok()).is_some_and(|hh| (17..=23).contains(&hh)))
        .filter_map(|(_, p)| *p)
        .max()
}

/// Middle of the two homes (or whichever one is set).
pub fn midpoint(homes: [Option<(f64, f64)>; 2]) -> Option<(f64, f64)> {
    match homes {
        [Some(a), Some(b)] => Some(((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)),
        [Some(a), None] | [None, Some(a)] => Some(a),
        [None, None] => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_evening_peak_only() {
        let h = Hourly {
            time: (0..24).map(|i| format!("2026-10-17T{i:02}:00")).collect(),
            precipitation_probability: (0..24).map(|i| Some(if i == 9 { 95 } else if i == 20 { 70 } else { 10 })).collect(),
        };
        assert_eq!(evening_max(&h), Some(70));
    }

    #[test]
    fn midpoint_of_homes() {
        assert_eq!(midpoint([Some((19.0, 73.0)), Some((19.2, 72.8))]).map(|(a, b)| (a.round(), b.round())), Some((19.0, 73.0)));
        assert_eq!(midpoint([None, Some((1.0, 2.0))]), Some((1.0, 2.0)));
        assert_eq!(midpoint([None, None]), None);
    }
}
