// src/Builtins/Static/datetime_object.rs
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
        // DateTime.now() - Returns current date and time
        base.register_method(Box::new(BuiltinMethod::new(
            "now".to_string(),
            0,
            DixType::Timestamp,
            |_args| Ok(DixValue::from_timestamp(Utc::now())),
            "Returns the current date and time".to_string(),
        )));

        // DateTime.today() - Returns today's date (time set to 00:00:00)
        base.register_method(Box::new(BuiltinMethod::new(
            "today".to_string(),
            0,
            DixType::Date,
            |_args| {
                let now = Utc::now();
                let date = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
                Ok(DixValue::from_date(DateTime::from_naive_utc_and_offset(date, Utc)))
            },
            "Returns today's date with time set to midnight".to_string(),
        )));

        // DateTime.utcNow() - Returns current UTC date and time
        base.register_method(Box::new(BuiltinMethod::new(
            "utcNow".to_string(),
            0,
            DixType::Timestamp,
            |_args| Ok(DixValue::from_timestamp(Utc::now())),
            "Returns the current UTC date and time".to_string(),
        )));

        // DateTime.parse(dateString) - Parse date string
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "parse".to_string(),
            1,
            DixType::Timestamp,
            |args| {
                let date_string = args[0].as_string();

                // Try parsing with chrono
                let parsed = date_string.parse::<DateTime<Utc>>()
                    .map_err(|_| format!("Cannot parse '{}' as a valid date", date_string))?;

                Ok(DixValue::from_timestamp(parsed))
            },
            "Parses a date string into a timestamp".to_string(),
            validation_helpers::first_is_string,
        )));

        // DateTime.parseExact(dateString, format) - Parse date with specific format
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "parseExact".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let date_string = args[0].as_string();
                let format = args[1].as_string();

                let parsed = chrono::NaiveDateTime::parse_from_str(&date_string, &format)
                    .map_err(|_| format!("Cannot parse '{}' using format '{}'", date_string, format))?;

                Ok(DixValue::from_timestamp(DateTime::from_naive_utc_and_offset(parsed, Utc)))
            },
            "Parses a date string with a specific format".to_string(),
            |args| validation_helpers::first_is_string(args) &&
                validation_helpers::argument_has_type(1, DixType::String, args),
        )));

        // DateTime.create(year, month, day) - Create date from components
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "create".to_string(),
            3,
            DixType::Date,
            |args| {
                let year = args[0].as_int();
                let month = args[1].as_int() as u32;
                let day = args[2].as_int() as u32;

                let date = NaiveDate::from_ymd_opt(year, month, day)
                    .ok_or_else(|| format!("Invalid date components: {}-{}-{}", year, month, day))?
                    .and_hms_opt(0, 0, 0)
                    .ok_or("Failed to create date")?;

                Ok(DixValue::from_date(DateTime::from_naive_utc_and_offset(date, Utc)))
            },
            "Creates a date from year, month, and day components".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.createTime(year, month, day, hour, minute, second) - Create timestamp
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "createTime".to_string(),
            6,
            DixType::Timestamp,
            |args| {
                let year = args[0].as_int();
                let month = args[1].as_int() as u32;
                let day = args[2].as_int() as u32;
                let hour = args[3].as_int() as u32;
                let minute = args[4].as_int() as u32;
                let second = args[5].as_int() as u32;

                let datetime = NaiveDate::from_ymd_opt(year, month, day)
                    .and_then(|d| d.and_hms_opt(hour, minute, second))
                    .ok_or_else(|| "Invalid datetime components".to_string())?;

                Ok(DixValue::from_timestamp(DateTime::from_naive_utc_and_offset(datetime, Utc)))
            },
            "Creates a timestamp from year, month, day, hour, minute, and second components".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.fromUnixTime(unixTimestamp) - Create from Unix timestamp
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "fromUnixTime".to_string(),
            1,
            DixType::Timestamp,
            |args| {
                let unix_time = args[0].as_double() as i64;
                let datetime = DateTime::from_timestamp(unix_time, 0)
                    .ok_or("Invalid Unix timestamp")?;
                Ok(DixValue::from_timestamp(datetime))
            },
            "Creates a timestamp from Unix time (seconds since 1970-01-01)".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.toUnixTime(timestamp) - Convert to Unix timestamp
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "toUnixTime".to_string(),
            1,
            DixType::Double,
            |args| {
                let datetime = args[0].as_datetime();
                let unix_time = datetime.timestamp() as f64;
                Ok(DixValue::from_double(unix_time))
            },
            "Converts a timestamp to Unix time (seconds since 1970-01-01)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp || args[0].get_type() == DixType::Date,
        )));

        // DateTime.isLeapYear(year) - Check if year is leap year
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "isLeapYear".to_string(),
            1,
            DixType::Bool,
            |args| {
                let year = args[0].as_int();
                let is_leap = NaiveDate::from_ymd_opt(year, 2, 29).is_some();
                Ok(DixValue::from_bool(is_leap))
            },
            "Checks if a year is a leap year".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.daysInMonth(year, month) - Get days in month
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "daysInMonth".to_string(),
            2,
            DixType::Int,
            |args| {
                let year = args[0].as_int();
                let month = args[1].as_int() as u32;

                if month < 1 || month > 12 {
                    return Err(format!("Invalid month: {}", month));
                }

                // Get first day of next month, then subtract 1 day
                let next_month = if month == 12 { 1 } else { month + 1 };
                let next_year = if month == 12 { year + 1 } else { year };

                let first_of_next = NaiveDate::from_ymd_opt(next_year, next_month, 1)
                    .ok_or("Invalid date")?;
                let last_of_month = first_of_next.pred_opt().ok_or("Failed to calculate last day")?;

                Ok(DixValue::from_int(last_of_month.day() as i32))
            },
            "Returns the number of days in the specified month and year".to_string(),
            validation_helpers::all_numeric,
        )));

        // DateTime.compare(date1, date2) - Compare two dates
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "compare".to_string(),
            2,
            DixType::Int,
            |args| {
                let date1 = args[0].as_datetime();
                let date2 = args[1].as_datetime();

                let result = if date1 < date2 {
                    -1
                } else if date1 > date2 {
                    1
                } else {
                    0
                };

                Ok(DixValue::from_int(result))
            },
            "Compares two dates (-1: first is earlier, 0: equal, 1: first is later)".to_string(),
            |args| (args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp) &&
                (args[1].get_type() == DixType::Date || args[1].get_type() == DixType::Timestamp),
        )));

        // DateTime.addDays(date, days) - Add days to date
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addDays".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let date = args[0].as_datetime();
                let days = args[1].as_double();

                let duration = Duration::milliseconds((days * 24.0 * 3600.0 * 1000.0) as i64);
                let result = date + duration;

                if args[0].get_type() == DixType::Date {
                    Ok(DixValue::from_date(result))
                } else {
                    Ok(DixValue::from_timestamp(result))
                }
            },
            "Adds the specified number of days to a date".to_string(),
            |args| (args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp) &&
                args[1].is_numeric(),
        )));

        // DateTime.addHours(timestamp, hours) - Add hours to timestamp
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addHours".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let datetime = args[0].as_datetime();
                let hours = args[1].as_double();

                let duration = Duration::milliseconds((hours * 3600.0 * 1000.0) as i64);
                let result = datetime + duration;

                Ok(DixValue::from_timestamp(result))
            },
            "Adds the specified number of hours to a timestamp".to_string(),
            |args| args[0].get_type() == DixType::Timestamp && args[1].is_numeric(),
        )));

        // DateTime.addMinutes(timestamp, minutes) - Add minutes to timestamp
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addMinutes".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let datetime = args[0].as_datetime();
                let minutes = args[1].as_double();

                let duration = Duration::milliseconds((minutes * 60.0 * 1000.0) as i64);
                let result = datetime + duration;

                Ok(DixValue::from_timestamp(result))
            },
            "Adds the specified number of minutes to a timestamp".to_string(),
            |args| args[0].get_type() == DixType::Timestamp && args[1].is_numeric(),
        )));

        // DateTime.addSeconds(timestamp, seconds) - Add seconds to timestamp
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addSeconds".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let datetime = args[0].as_datetime();
                let seconds = args[1].as_double();

                let duration = Duration::milliseconds((seconds * 1000.0) as i64);
                let result = datetime + duration;

                Ok(DixValue::from_timestamp(result))
            },
            "Adds the specified number of seconds to a timestamp".to_string(),
            |args| args[0].get_type() == DixType::Timestamp && args[1].is_numeric(),
        )));

        // DateTime.format(date, format) - Format date with custom format
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "format".to_string(),
            2,
            DixType::String,
            |args| {
                let datetime = args[0].as_datetime();
                let format = args[1].as_string();

                let result = datetime.format(&format).to_string();
                Ok(DixValue::from_string(result))
            },
            "Formats a date using the specified format string".to_string(),
            |args| (args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp) &&
                args[1].get_type() == DixType::String,
        )));

        // DateTime.dayOfWeek(date) - Get day of week (0=Sunday, 6=Saturday)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "dayOfWeek".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                let weekday = datetime.weekday().num_days_from_sunday();
                Ok(DixValue::from_int(weekday as i32))
            },
            "Gets the day of week (0=Sunday, 1=Monday, ..., 6=Saturday)".to_string(),
            |args| args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.dayOfYear(date) - Get day of year (1-366)
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "dayOfYear".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                let day_of_year = datetime.ordinal();
                Ok(DixValue::from_int(day_of_year as i32))
            },
            "Gets the day of the year (1-366)".to_string(),
            |args| args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.year(date) - Get year component
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "year".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_int(datetime.year()))
            },
            "Gets the year component of a date".to_string(),
            |args| args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.month(date) - Get month component
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "month".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_int(datetime.month() as i32))
            },
            "Gets the month component of a date (1-12)".to_string(),
            |args| args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.day(date) - Get day component
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "day".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_int(datetime.day() as i32))
            },
            "Gets the day component of a date (1-31)".to_string(),
            |args| args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.hour(timestamp) - Get hour component
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "hour".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_int(datetime.hour() as i32))
            },
            "Gets the hour component of a timestamp (0-23)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.minute(timestamp) - Get minute component
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "minute".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_int(datetime.minute() as i32))
            },
            "Gets the minute component of a timestamp (0-59)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.second(timestamp) - Get second component
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "second".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                Ok(DixValue::from_int(datetime.second() as i32))
            },
            "Gets the second component of a timestamp (0-59)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.millisecond(timestamp) - Get millisecond component
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "millisecond".to_string(),
            1,
            DixType::Int,
            |args| {
                let datetime = args[0].as_datetime();
                let nanos = datetime.timestamp_subsec_nanos();
                Ok(DixValue::from_int((nanos / 1_000_000) as i32))
            },
            "Gets the millisecond component of a timestamp (0-999)".to_string(),
            |args| args[0].get_type() == DixType::Timestamp,
        )));

        // DateTime.addYears(date, years) - Add years to date
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addYears".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let date = args[0].as_datetime();
                let years = args[1].as_int();

                let new_year = date.year() + years;
                let result = date.with_year(new_year)
                    .ok_or("Invalid year addition")?;

                if args[0].get_type() == DixType::Date {
                    Ok(DixValue::from_date(result))
                } else {
                    Ok(DixValue::from_timestamp(result))
                }
            },
            "Adds the specified number of years to a date".to_string(),
            |args| (args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp) &&
                args[1].is_numeric(),
        )));

        // DateTime.addMonths(date, months) - Add months to date
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "addMonths".to_string(),
            2,
            DixType::Timestamp,
            |args| {
                let date = args[0].as_datetime();
                let months = args[1].as_int();

                let total_months = date.month() as i32 + months;
                let new_month = ((total_months - 1).rem_euclid(12) + 1) as u32;
                let year_offset = (total_months - 1).div_euclid(12);
                let new_year = date.year() + year_offset;

                // Handle day overflow (e.g., Jan 31 + 1 month = Feb 28/29)
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
            |args| (args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp) &&
                args[1].is_numeric(),
        )));

        // DateTime.subtract(date1, date2) - Get time difference
        base.register_method(Box::new(BuiltinMethod::new_with_validator(
            "subtract".to_string(),
            2,
            DixType::Double,
            |args| {
                let date1 = args[0].as_datetime();
                let date2 = args[1].as_datetime();

                let difference = date1 - date2;
                let days = difference.num_milliseconds() as f64 / (24.0 * 3600.0 * 1000.0);

                Ok(DixValue::from_double(days))
            },
            "Calculates the difference between two dates in days".to_string(),
            |args| (args[0].get_type() == DixType::Date || args[0].get_type() == DixType::Timestamp) &&
                (args[1].get_type() == DixType::Date || args[1].get_type() == DixType::Timestamp),
        )));
    }
}

impl Default for DateTimeObject {
    fn default() -> Self {
        Self::new()
    }
}

impl IStaticObject for DateTimeObject {
    fn name(&self) -> &str {
        self.base.name()
    }

    fn call_method(&self, method_name: &str, args: &[DixValue]) -> Result<DixValue, String> {
        self.base.call_method(method_name, args)
    }

    fn has_method(&self, method_name: &str) -> bool {
        self.base.has_method(method_name)
    }

    fn get_method_names(&self) -> Vec<String> {
        self.base.get_method_names()
    }

    fn get_method(&self, method_name: &str) -> Option<&dyn IBuiltinMethod> {
        self.base.get_method(method_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime_now() {
        let dt = DateTimeObject::new();
        let result = dt.call_method("now", &[]).unwrap();
        assert_eq!(result.get_type(), DixType::Timestamp);
    }

    #[test]
    fn test_datetime_create() {
        let dt = DateTimeObject::new();
        let result = dt.call_method(
            "create",
            &[
                DixValue::from_int(2025),
                DixValue::from_int(1),
                DixValue::from_int(23),
            ],
        ).unwrap();
        assert_eq!(result.get_type(), DixType::Date);
    }

    #[test]
    fn test_datetime_is_leap_year() {
        let dt = DateTimeObject::new();
        let result = dt.call_method("isLeapYear", &[DixValue::from_int(2024)]).unwrap();
        assert!(result.as_bool());

        let result = dt.call_method("isLeapYear", &[DixValue::from_int(2025)]).unwrap();
        assert!(!result.as_bool());
    }
}