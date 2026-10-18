//! Rough one-way travel estimates for Mumbai, from the road distance Joy already
//! computes. No live prices: Uber/Ola don't offer public fare APIs, so these are
//! ballparks (shown as "~₹") with a link to check the real fare.

/// App cab (Uber Go / Ola Mini class), roughly as of 2026.
const CAB_BASE: f64 = 50.0;
const CAB_PER_KM: f64 = 14.0;
const CAB_PER_MIN: f64 = 1.5;
const CAB_MIN_FARE: f64 = 100.0;
/// Typical Mumbai road speed including traffic.
const CAB_KMPH: f64 = 22.0;

/// Mumbai local train, second class, plus a short hop to and from stations.
const TRAIN_KMPH: f64 = 32.0;
const TRAIN_STATION_MINS: f64 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Estimate {
    pub rupees: i64,
    pub minutes: i64,
}

pub fn cab(km: f64) -> Estimate {
    let minutes = (km / CAB_KMPH * 60.0).max(5.0);
    let fare = (CAB_BASE + km * CAB_PER_KM + minutes * CAB_PER_MIN).max(CAB_MIN_FARE);
    Estimate { rupees: round_to(fare, 10), minutes: round_to(minutes, 5) }
}

/// Second-class local fare bands (₹5 per ~15 km, starting at ₹5).
pub fn train(km: f64) -> Estimate {
    let fare = 5.0 * ((km / 15.0).ceil().max(1.0));
    let minutes = km / TRAIN_KMPH * 60.0 + TRAIN_STATION_MINS;
    Estimate { rupees: fare as i64, minutes: round_to(minutes, 5) }
}

fn round_to(v: f64, step: i64) -> i64 {
    ((v / step as f64).round() as i64 * step).max(step)
}

/// "35 min" or "1 h 20 min".
pub fn duration_label(minutes: i64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cab_fares_scale_with_distance() {
        let short = cab(3.0);
        let far = cab(30.0);
        assert_eq!(short.rupees, 100, "{short:?}");
        assert_eq!(far.rupees, 590, "{far:?}");
        assert_eq!(far.minutes, 80);
        assert_eq!(cab(0.5).rupees, 100, "minimum fare");
    }

    #[test]
    fn train_is_cheap_but_slower_for_short_hops() {
        assert_eq!(train(10.0).rupees, 5);
        assert_eq!(train(30.0).rupees, 10);
        assert_eq!(train(31.0).rupees, 15);
        assert!(train(3.0).minutes > cab(3.0).minutes);
    }

    #[test]
    fn labels() {
        assert_eq!(duration_label(35), "35 min");
        assert_eq!(duration_label(60), "1 h");
        assert_eq!(duration_label(80), "1 h 20 min");
    }
}
