use crate::error::{Result, SlotdError};
use crate::sbatch::parse_time_limit_secs;

pub(crate) fn now_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn parse_begin_time(value: &str) -> Result<i64> {
    let trimmed = value.trim();
    if let Some(offset) = trimmed.strip_prefix("now+") {
        return Ok(now_ts().saturating_add(parse_time_limit_secs(offset)? as i64));
    }
    parse_time_filter(trimmed)
}

pub(crate) fn parse_time_filter(value: &str) -> Result<i64> {
    if let Ok(ts) = value.parse::<i64>() {
        return Ok(ts);
    }
    if value.eq_ignore_ascii_case("now") {
        return Ok(now_ts());
    }
    if let Some((year, month, day, hour, minute, second)) = parse_datetime_parts(value) {
        return datetime_to_epoch(year, month, day, hour, minute, second);
    }
    Err(SlotdError::from(format!(
        "unsupported time format: {value}"
    )))
}

pub(crate) fn format_timestamp(value: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_epoch(value);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}")
}

pub(crate) fn format_duration_secs(seconds: i64) -> String {
    let seconds = seconds.max(0) as u64;
    let days = seconds / 86_400;
    let rem = seconds % 86_400;
    let hours = rem / 3_600;
    let minutes = (rem % 3_600) / 60;
    let secs = rem % 60;
    if days > 0 {
        format!("{days}-{hours:02}:{minutes:02}:{secs:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{secs:02}")
    }
}

fn parse_datetime_parts(value: &str) -> Option<(i32, u32, u32, u32, u32, u32)> {
    if let Some((date, time)) = value.split_once('T').or_else(|| value.split_once(' ')) {
        let (year, month, day) = parse_date(date)?;
        let (hour, minute, second) = parse_time(time)?;
        return Some((year, month, day, hour, minute, second));
    }

    let (year, month, day) = parse_date(value)?;
    Some((year, month, day, 0, 0, 0))
}

fn parse_date(value: &str) -> Option<(i32, u32, u32)> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts.next()?.parse().ok()?;
    let day = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((year, month, day))
}

fn parse_time(value: &str) -> Option<(u32, u32, u32)> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse().ok()?;
    let minute = parts.next()?.parse().ok()?;
    let second = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((hour, minute, second))
}

fn datetime_to_epoch(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Result<i64> {
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(SlotdError::from("invalid date or time"));
    }

    let days = days_from_civil(year, month, day).ok_or_else(|| SlotdError::from("invalid date"))?;
    Ok(days * 86_400 + hour as i64 * 3_600 + minute as i64 * 60 + second as i64)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if day > days_in_month(year, month)? {
        return None;
    }

    let mut year = year as i64;
    let month = month as i64;
    let day = day as i64;
    year -= if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn days_in_month(year: i32, month: u32) -> Option<u32> {
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    };
    Some(days)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn civil_from_epoch(timestamp: i64) -> (i32, u32, u32, u32, u32, u32) {
    let days = timestamp.div_euclid(86_400);
    let secs = timestamp.rem_euclid(86_400) as u32;
    let (year, month, day) = civil_from_days(days);
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    (year, month, day, hour, minute, second)
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if month <= 2 { 1 } else { 0 };
    (year as i32, month as u32, day as u32)
}
