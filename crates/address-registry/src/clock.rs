//! UTC timestamps without a date library.
//!
//! The registry records when it was generated, in the same ISO-8601 form the
//! upstream JSON uses. `SystemTime` plus the civil-from-days algorithm is
//! enough, and it keeps the tool free of a date dependency.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> String {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    from_unix(elapsed.as_secs(), elapsed.subsec_millis())
}

fn from_unix(seconds: u64, millis: u32) -> String {
    let seconds = seconds as i64;
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        second_of_day / 3_600,
        (second_of_day % 3_600) / 60,
        second_of_day % 60,
    )
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a Gregorian date.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    (
        if month <= 2 { year + 1 } else { year },
        month as u32,
        day as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_1970() {
        assert_eq!(from_unix(0, 0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn a_leap_day_is_the_29th() {
        // 2024-02-29T12:00:00.000Z
        assert_eq!(from_unix(1_709_208_000, 0), "2024-02-29T12:00:00.000Z");
    }

    #[test]
    fn the_first_imports_timestamp_round_trips() {
        // 2026-11-19T00:57:48.132Z, the `generated_at` on the reference import.
        assert_eq!(from_unix(1_795_049_868, 132), "2026-11-19T00:57:48.132Z");
    }
}
