// dixscript/src/Builtins/Static/datetime_object.rs
//! DateTime static object implementation for DixScript
//! Provides date and time functions like now, today, parse, etc.

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod, validation_helpers};
use crate::Builtins::Static::{IStaticObject, StaticObjectBase};
use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc, Duration};

/// DateTime static object implementation
pub struct DateTimeObject {
    base: StaticObjectBase,
}

impl DateTimeObject {
    pub fn new() -> Self {
        let mut base = StaticObjectBase::new("DateTime".to_string());
        Self::initialize_methods(&mut base);
        DateTimeObject { base }
    }

    fn initialize_methods(base: &mut StaticObjectBase) {
        // DateTime.now()
        base.register_method(Box::new(BuiltinMethod::new(
            "now".to_string(),
            0,
            DixType::Timestamp,
            |_| Ok(DixValue::from_timestamp(Utc::now())),
            "Returns the current date and time".to_string(),
        )));

        // DateTime.today()
        base.register_method(Box::new(BuiltinMethod::new(
            "today".to_string(),
            0,
            DixType::Date,
            |_| {
                let now  = Utc::now();
                let date = now.date_naive()
                    .and_hms_opt(0, 0, 0)
                    .unwrap();
                Ok(DixValue::from_date(DateTime::from_naive_utc_and_offset(date, Utc)))
            },
            "Returns today's date with time set to midnight".to_string(),
        )));

        // DateTime.utcNow()
        base.register_method(Box::new(BuiltinMethod::new(
            "utcNow".to_string(),
            0,
            DixType::Timestamp,
            |_| Ok(DixValue::from_timestamp(Utc::now())),
            "Returns the current UTC date and time".to_string(),
        )));

        // DateTime.parse(dateString)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "parse".to_string(),
            1,
            DixType::Timestamp,
            |args| {
                let s = args[0].as_string();
                let parsed = s.parse::<DateTime<Utc>>()
                    .map_err(|_| format!("Cannot parse '{}' as a valid date", s))?;
                Ok(DixValue::from_timestamp(parsed))
            },
            "Parses a date string into a timestamp".to_string(),
            validation_helpers::first_is_string,
        )));

        // DateTime.parseExact(dateString, format)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "parseExact".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let s      = args[0].as_string();
                let format = args[1].as_string();
                let parsed = chrono::NaiveDateTime::parse_from_str(&s, &format)
                    .map_err(|_| format!("Cannot parse '{}' using format '{}'", s, format))?;
                Ok(DixValue::from_timestamp(DateTime::from_naive_utc_and_offset(parsed, Utc)))
            },
            "Parses a date string with a specific format".to_string(),
            |args| {
                validation_helpers::first_is_string(args)
                    && validation_helpers::argument_has_type(1, DixType::String, args)
            },
        )));

        // DateTime.create(year, month, day)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "create".to_string(),
            3,
            DixType::Date,
            |args| {
                let year  = args[0].as_int();
                let month = args[1].as_int() as u32;
                let day   = args[2].as_int() as u32;
                let date  = NaiveDate::from_ymd_opt(year, month, day)
                    .ok_or_else(|| format!("Invalid date components: {}-{}-{}", year, month, day))?
                    .and_hms_opt(0, 0, 0)
                    .ok_or("Failed to create date")?;
                Ok(DixValue::from_date(DateTime::from_naive_utc_and_offset(date, Utc)))
            },
            "Creates a date from year, month, and day components".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.createTime(year, month, day, hour, minute, second)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "createTime".to_string(),
            6,
            DixType::Timestamp,
            |args| {
                let year   = args[0].as_int();
                let month  = args[1].as_int() as u32;
                let day    = args[2].as_int() as u32;
                let hour   = args[3].as_int() as u32;
                let minute = args[4].as_int() as u32;
                let second = args[5].as_int() as u32;
                let dt     = NaiveDate::from_ymd_opt(year, month, day)
                    .and_then(|d| d.and_hms_opt(hour, minute, second))
                    .ok_or("Invalid datetime components")?;
                Ok(DixValue::from_timestamp(DateTime::from_naive_utc_and_offset(dt, Utc)))
            },
            "Creates a timestamp from year, month, day, hour, minute, and second components".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.fromUnixTime(unixTimestamp) — accepts Int or Long.
        //
        // FIX: was `as_double() as i64`, which loses precision for values beyond
        // 2^53 (year ~287,396). Now uses `as_long()` which is lossless for both
        // Int (widened) and Long (direct).
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "fromUnixTime".to_string(),
            1,
            DixType::Timestamp,
            |args| {
                // as_long() handles Int (widens), Long (direct), Float/Double (truncating cast)
                let unix_secs = args[0].as_long();
                let datetime  = DateTime::from_timestamp(unix_secs, 0)
                    .ok_or_else(|| format!("Invalid Unix timestamp: {}", unix_secs))?;
                Ok(DixValue::from_timestamp(datetime))
            },
            "Creates a timestamp from Unix time (seconds since 1970-01-01). \
             Accepts int or long — use long for dates beyond year 2038.".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.toUnixTime(timestamp) — returns Long for lossless precision.
        //
        // FIX: was Double. Unix epoch seconds are whole integers and fit in i64
        // for any date within ±292 billion years, so Long is the right return
        // type. Callers that need fractional seconds should use toUnixMillis.
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "toUnixTime".to_string(),
            1,
            DixType::Long,
            |args| {
                let datetime  = args[0].as_datetime();
                let unix_secs = datetime.timestamp();
                Ok(DixValue::from_long(unix_secs))
            },
            "Converts a timestamp to Unix time (whole seconds since 1970-01-01) as a long.".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Timestamp | DixType::Date)
            },
        )));

        // DateTime.toUnixMillis(timestamp) — milliseconds since epoch as Long.
        //
        // NEW: companion to toUnixTime for callers that need sub-second precision
        // without converting to Double. Always returns Long.
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "toUnixMillis".to_string(),
            1,
            DixType::Long,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_long(datetime.timestamp_millis()))
            },
            "Converts a timestamp to milliseconds since 1970-01-01 as a long.".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Timestamp | DixType::Date)
            },
        )));

        // DateTime.isLeapYear(year)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "isLeapYear".to_string(),
            1,
            DixType::Bool,
            |args| {
                let year    = args[0].as_int();
                let is_leap = NaiveDate::from_ymd_opt(year, 2, 29).is_some();
                Ok(DixValue::from_bool(is_leap))
            },
            "Checks if a year is a leap year".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.daysInMonth(year, month)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "daysInMonth".to_string(),
            2,
            DixType::Int,
            |args| {
                let year  = args[0].as_int();
                let month = args[1].as_int() as u32;
                if !(1..=12).contains(&month) {
                    return Err(format!("Invalid month: {}", month));
                }
                let next_month = if month == 12 { 1 } else { month + 1 };
                let next_year  = if month == 12 { year + 1 } else { year };
                let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)
                    .ok_or("Invalid date")?;
                let last_this  = first_next.pred_opt()
                    .ok_or("Failed to calculate last day")?;
                Ok(DixValue::from_int(last_this.day() as i32))
            },
            "Returns the number of days in the specified month and year".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.compare(date1, date2)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "compare".to_string(),
            2,
            DixType::Int,
            |args| {
                let d1     = args[0].as_datetime();
                let d2     = args[1].as_datetime();
                let result = if d1 < d2 { -1 } else if d1 > d2 { 1 } else { 0 };
                Ok(DixValue::from_int(result))
            },
            "Compares two dates (-1: first is earlier, 0: equal, 1: first is later)".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Date | DixType::Timestamp)
                    && matches!(args[1].get_type(), DixType::Date | DixType::Timestamp)
            },
        )));

        // DateTime.addDays(date, days)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addDays".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let date     = args[0].as_datetime();
                let days     = args[1].as_double();
                let duration = Duration::milliseconds((days * 24.0 * 3600.0 * 1000.0) as i64);
                let result   = date + duration;
                if args[0].get_type() == DixType::Date {
                    Ok(DixValue::from_date(result))
                } else {
                    Ok(DixValue::from_timestamp(result))
                }
            },
            "Adds the specified number of days to a date".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Date | DixType::Timestamp)
                    && args[1].is_numeric()
            },
        )));

        // DateTime.addHours(timestamp, hours)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addHours".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let datetime = args[0].as_datetime();
                let hours    = args[1].as_double();
                let duration = Duration::milliseconds((hours * 3600.0 * 1000.0) as i64);
                Ok(DixValue::from_timestamp(datetime + duration))
            },
            "Adds the specified number of hours to a timestamp".to_string(),
            |args| {
                args[0].get_type() == DixType::Timestamp && args[1].is_numeric()
            },
        )));

        // DateTime.addMinutes(timestamp, minutes)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addMinutes".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let datetime = args[0].as_datetime();
                let minutes  = args[1].as_double();
                let duration = Duration::milliseconds((minutes * 60.0 * 1000.0) as i64);
                Ok(DixValue::from_timestamp(datetime + duration))
            },
            "Adds the specified number of minutes to a timestamp".to_string(),
            |args| {
                args[0].get_type() == DixType::Timestamp && args[1].is_numeric()
            },
        )));

        // DateTime.addSeconds(timestamp, seconds)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addSeconds".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let datetime = args[0].as_datetime();
                let seconds  = args[1].as_double();
                let duration = Duration::milliseconds((seconds * 1000.0) as i64);
                Ok(DixValue::from_timestamp(datetime + duration))
            },
            "Adds the specified number of seconds to a timestamp".to_string(),
            |args| {
                args[0].get_type() == DixType::Timestamp && args[1].is_numeric()
            },
        )));

        // DateTime.format(date, format)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "format".to_string(),
            2,
            DixType::String,
            |args| {
                let datetime = args[0].as_datetime();
                let format   = args[1].as_string();
                Ok(DixValue::from_string(datetime.format(&format).to_string()))
            },
            "Formats a date using the specified format string".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Date | DixType::Timestamp)
                    && args[1].get_type() == DixType::String
            },
        )));

        // DateTime.dayOfWeek(date) — 0=Sunday … 6=Saturday
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "dayOfWeek".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_int(datetime.weekday().num_days_from_sunday() as i32))
            },
            "Gets the day of week (0=Sunday, 1=Monday, ..., 6=Saturday)".to_string(),
            |args| matches!(args[0].get_type(), DixType::Date | DixType::Timestamp),
        )));

        // DateTime.dayOfYear(date) — 1-366
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "dayOfYear".to_string(),
            1,
            DixType::Int,
            |args| {
                Ok(DixValue::from_int(args[0].as_datetime().ordinal() as i32))
            },
            "Gets the day of the year (1-366)".to_string(),
            |args| matches!(args[0].get_type(), DixType::Date | DixType::Timestamp),
        )));

        // DateTime.year(date)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "year".to_string(),
            1,
            DixType::Int,
            |args| Ok(DixValue::from_int(args[0].as_datetime().year())),
            "Gets the year component of a date".to_string(),
            |args| matches!(args[0].get_type(), DixType::Date | DixType::Timestamp),
        )));

        // DateTime.month(date) — 1-12
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "month".to_string(),
            1,
            DixType::Int,
            |args| Ok(DixValue::from_int(args[0].as_datetime().month() as i32)),
            "Gets the month component of a date (1-12)".to_string(),
            |args| matches!(args[0].get_type(), DixType::Date | DixType::Timestamp),
        )));

        // DateTime.day(date) — 1-31
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "day".to_string(),
            1,
            DixType::Int,
            |args| Ok(DixValue::from_int(args[0].as_datetime().day() as i32)),
            "Gets the day component of a date (1-31)".to_string(),
            |args| matches!(args[0].get_type(), DixType::Date | DixType::Timestamp),
        )));

        // DateTime.hour(timestamp) — 0-23
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "hour".to_string(),
            1,
            DixType::Int,
            |args| Ok(DixValue::from_int(args[0].as_datetime().hour() as i32)),
            "Gets the hour component of a timestamp (0-23)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.minute(timestamp) — 0-59
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "minute".to_string(),
            1,
            DixType::Int,
            |args| Ok(DixValue::from_int(args[0].as_datetime().minute() as i32)),
            "Gets the minute component of a timestamp (0-59)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.second(timestamp) — 0-59
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "second".to_string(),
            1,
            DixType::Int,
            |args| Ok(DixValue::from_int(args[0].as_datetime().second() as i32)),
            "Gets the second component of a timestamp (0-59)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.millisecond(timestamp) — 0-999
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "millisecond".to_string(),
            1,
            DixType::Int,
            |args| {
                let nanos = args[0].as_datetime().timestamp_subsec_nanos();
                Ok(DixValue::from_int((nanos / 1_000_000) as i32))
            },
            "Gets the millisecond component of a timestamp (0-999)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.addYears(date, years)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addYears".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let date     = args[0].as_datetime();
                let years    = args[1].as_int();
                let new_year = date.year() + years;
                let result   = date.with_year(new_year)
                    .ok_or("Invalid year addition")?;
                if args[0].get_type() == DixType::Date {
                    Ok(DixValue::from_date(result))
                } else {
                    Ok(DixValue::from_timestamp(result))
                }
            },
            "Adds the specified number of years to a date".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Date | DixType::Timestamp)
                    && args[1].is_numeric()
            },
        )));

        // DateTime.addMonths(date, months)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addMonths".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let date         = args[0].as_datetime();
                let months       = args[1].as_int();
                let total_months = date.month() as i32 + months;
                let new_month    = ((total_months - 1).rem_euclid(12) + 1) as u32;
                let year_offset  = (total_months - 1).div_euclid(12);
                let new_year     = date.year() + year_offset;

                // Clamp day to last valid day of target month
                let max_day = NaiveDate::from_ymd_opt(new_year, new_month + 1, 1)
                    .or_else(|| NaiveDate::from_ymd_opt(new_year + 1, 1, 1))
                    .map(|d| d.pred_opt().map(|p| p.day()).unwrap_or(28))
                    .unwrap_or(28);

                let new_day = date.day().min(max_day);

                let result = date.with_year(new_year)
                    .and_then(|d| d.with_month(new_month))
                    .and_then(|d| d.with_day(new_day))
                    .ok_or("Invalid month addition")?;

                if args[0].get_type() == DixType::Date {
                    Ok(DixValue::from_date(result))
                } else {
                    Ok(DixValue::from_timestamp(result))
                }
            },
            "Adds the specified number of months to a date".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Date | DixType::Timestamp)
                    && args[1].is_numeric()
            },
        )));

        // DateTime.subtract(date1, date2) — difference in days as Double
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "subtract".to_string(),
            2,
            DixType::Double,
            |args| {
                let d1         = args[0].as_datetime();
                let d2         = args[1].as_datetime();
                let difference = d1 - d2;
                let days       = difference.num_milliseconds() as f64
                    / (24.0 * 3600.0 * 1000.0);
                Ok(DixValue::from_double(days))
            },
            "Calculates the difference between two dates in days (fractional)".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Date | DixType::Timestamp)
                    && matches!(args[1].get_type(), DixType::Date | DixType::Timestamp)
            },
        )));

        // DateTime.subtractMillis(date1, date2) — difference in milliseconds as Long
        //
        // NEW: Long-precision companion to subtract() for when fractional days
        // aren't what you want (e.g. performance timing, precise intervals).
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "subtractMillis".to_string(),
            2,
            DixType::Long,
            |args| {
                let d1     = args[0].as_datetime();
                let d2     = args[1].as_datetime();
                let millis = (d1 - d2).num_milliseconds();
                Ok(DixValue::from_long(millis))
            },
            "Calculates the difference between two timestamps in milliseconds as a long.".to_string(),
            |args| {
                matches!(args[0].get_type(), DixType::Date | DixType::Timestamp)
                    && matches!(args[1].get_type(), DixType::Date | DixType::Timestamp)
            },
        )));
    }
}

impl Default for DateTimeObject {
    fn default() -> Self { Self::new() }
}

impl IStaticObject for DateTimeObject {
    fn name(&self) -> &str { self.base.name() }

    fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
        self.base.call_method(method_name, args)
    }

    fn has_method(&self, method_name: &str) -> bool { self.base.has_method(method_name) }

    fn get_method_names(&self) -> Vec<String> { self.base.get_method_names() }

    fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod> {
        self.base.get_method(method_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime_now() {
        let dt     = DateTimeObject::new();
        let result = dt.call_method("now", &[]).unwrap();
        assert_eq!(result.get_type(), DixType::Timestamp);
    }

    #[test]
    fn test_datetime_today() {
        let dt     = DateTimeObject::new();
        let result = dt.call_method("today", &[]).unwrap();
        assert_eq!(result.get_type(), DixType::Date);
    }

    #[test]
    fn test_datetime_create() {
        let dt     = DateTimeObject::new();
        let result = dt.call_method("create", &[
            DixValue::from_int(2025),
            DixValue::from_int(1),
            DixValue::from_int(23),
        ]).unwrap();
        assert_eq!(result.get_type(), DixType::Date);
    }

    #[test]
    fn test_datetime_is_leap_year() {
        let dt = DateTimeObject::new();

        let leap = dt.call_method("isLeapYear", &[DixValue::from_int(2024)]).unwrap();
        assert!(leap.as_bool());

        let not_leap = dt.call_method("isLeapYear", &[DixValue::from_int(2025)]).unwrap();
        assert!(!not_leap.as_bool());
    }

    #[test]
    fn test_from_unix_time_returns_timestamp() {
        let dt     = DateTimeObject::new();
        let result = dt.call_method("fromUnixTime", &[DixValue::from_long(0_i64)]).unwrap();
        assert_eq!(result.get_type(), DixType::Timestamp);
    }

    #[test]
    fn test_from_unix_time_with_int_input() {
        // Int input must work too (widened via as_long())
        let dt     = DateTimeObject::new();
        let result = dt.call_method("fromUnixTime", &[DixValue::from_int(0)]).unwrap();
        assert_eq!(result.get_type(), DixType::Timestamp);
    }

    #[test]
    fn test_from_unix_time_with_long_large_value() {
        // Year 3000 — Unix timestamp 32_503_680_000 which overflows i32
        let dt        = DateTimeObject::new();
        let ts: i64   = 32_503_680_000_i64;
        let result    = dt.call_method("fromUnixTime", &[DixValue::from_long(ts)]).unwrap();
        assert_eq!(result.get_type(), DixType::Timestamp);
    }

    #[test]
    fn test_to_unix_time_returns_long() {
        let dt        = DateTimeObject::new();
        let now       = dt.call_method("now", &[]).unwrap();
        let unix_time = dt.call_method("toUnixTime", &[now]).unwrap();
        assert_eq!(unix_time.get_type(), DixType::Long);
        assert!(unix_time.as_long() > 0);
    }

    #[test]
    fn test_to_unix_millis_returns_long() {
        let dt     = DateTimeObject::new();
        let now    = dt.call_method("now", &[]).unwrap();
        let millis = dt.call_method("toUnixMillis", &[now]).unwrap();
        assert_eq!(millis.get_type(), DixType::Long);
        // Millis should be ~1000x the seconds value
        let secs = dt.call_method("toUnixTime", &[
            dt.call_method("now", &[]).unwrap()
        ]).unwrap();
        let ratio = millis.as_long() as f64 / secs.as_long() as f64;
        assert!(ratio > 900.0 && ratio < 1100.0);
    }

    #[test]
    fn test_subtract_millis_returns_long() {
        let dt = DateTimeObject::new();
        let t1 = dt.call_method("createTime", &[
            DixValue::from_int(2025), DixValue::from_int(1), DixValue::from_int(1),
            DixValue::from_int(0),    DixValue::from_int(0), DixValue::from_int(1),
        ]).unwrap();
        let t2 = dt.call_method("createTime", &[
            DixValue::from_int(2025), DixValue::from_int(1), DixValue::from_int(1),
            DixValue::from_int(0),    DixValue::from_int(0), DixValue::from_int(0),
        ]).unwrap();
        let diff = dt.call_method("subtractMillis", &[t1, t2]).unwrap();
        assert_eq!(diff.get_type(), DixType::Long);
        assert_eq!(diff.as_long(), 1_000_i64); // exactly 1 second = 1000 ms
    }

    #[test]
    fn test_round_trip_unix_time() {
        // create a known timestamp, convert to unix, convert back, compare
        let dt = DateTimeObject::new();
        let original = dt.call_method("createTime", &[
            DixValue::from_int(2025), DixValue::from_int(6), DixValue::from_int(15),
            DixValue::from_int(12),   DixValue::from_int(0), DixValue::from_int(0),
        ]).unwrap();
        let unix    = dt.call_method("toUnixTime", &[original.clone()]).unwrap();
        let back    = dt.call_method("fromUnixTime", &[unix]).unwrap();
        // Seconds should match (millisecond precision not guaranteed after round-trip)
        let orig_s  = dt.call_method("second", &[original]).unwrap().as_int();
        let back_s  = dt.call_method("second", &[back]).unwrap().as_int();
        assert_eq!(orig_s, back_s);
    }

    #[test]
    fn test_days_in_month_february_leap() {
        let dt     = DateTimeObject::new();
        let result = dt.call_method("daysInMonth", &[
            DixValue::from_int(2024),
            DixValue::from_int(2),
        ]).unwrap();
        assert_eq!(result.as_int(), 29);
    }

    #[test]
    fn test_days_in_month_february_non_leap() {
        let dt     = DateTimeObject::new();
        let result = dt.call_method("daysInMonth", &[
            DixValue::from_int(2025),
            DixValue::from_int(2),
        ]).unwrap();
        assert_eq!(result.as_int(), 28);
    }

    #[test]
    fn test_compare_dates() {
        let dt = DateTimeObject::new();
        let d1 = dt.call_method("create", &[
            DixValue::from_int(2024), DixValue::from_int(1), DixValue::from_int(1),
        ]).unwrap();
        let d2 = dt.call_method("create", &[
            DixValue::from_int(2025), DixValue::from_int(1), DixValue::from_int(1),
        ]).unwrap();
        let earlier = dt.call_method("compare", &[d1.clone(), d2.clone()]).unwrap();
        assert_eq!(earlier.as_int(), -1);
        let later = dt.call_method("compare", &[d2, d1]).unwrap();
        assert_eq!(later.as_int(), 1);
    }

    #[test]
    fn test_add_days() {
        let dt   = DateTimeObject::new();
        let base = dt.call_method("create", &[
            DixValue::from_int(2025), DixValue::from_int(1), DixValue::from_int(1),
        ]).unwrap();
        let result = dt.call_method("addDays", &[base, DixValue::from_int(10)]).unwrap();
        let day    = dt.call_method("day", &[result]).unwrap();
        assert_eq!(day.as_int(), 11);
    }
}
