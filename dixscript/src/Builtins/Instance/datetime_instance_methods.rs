// dixscript/src/Builtins/Instance/datetime_instance_methods.rs
//! Instance methods for Timestamp and Date values.
//!
//! These complement the static `DateTime` object and allow instance-style
//! chaining:  `DateTime.now().addDays(7).addHours(2).format("yyyy-MM-dd")`
//!
//! Every method receives the instance as `args[0]` (prepended by
//! `call_instance_method`). The `parameter_count` therefore counts self:
//!   no-arg method  → parameter_count = 1   (self only)
//!   one-arg method → parameter_count = 2   (self + arg)

use crate::Builtins::Core::{BuiltinMethod, DixType, DixValue, IBuiltinMethod};
use chrono::{Datelike, Duration, Timelike};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// Timestamp instance methods
// ═══════════════════════════════════════════════════════════════════════════

pub fn get_timestamp_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut m: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    // ── Time-offset mutations ─────────────────────────────────────────────

    m.insert("addDays".to_string(), Box::new(BuiltinMethod::new(
        "addDays".to_string(), 2, DixType::Timestamp,
        |args| {
            let dt  = args[0].as_datetime();
            let dur = Duration::milliseconds((args[1].as_double() * 86_400_000.0) as i64);
            Ok(DixValue::from_timestamp(dt + dur))
        },
        "Returns a new Timestamp offset by N days".to_string(),
    )));

    m.insert("addHours".to_string(), Box::new(BuiltinMethod::new(
        "addHours".to_string(), 2, DixType::Timestamp,
        |args| {
            let dt  = args[0].as_datetime();
            let dur = Duration::milliseconds((args[1].as_double() * 3_600_000.0) as i64);
            Ok(DixValue::from_timestamp(dt + dur))
        },
        "Returns a new Timestamp offset by N hours".to_string(),
    )));

    m.insert("addMinutes".to_string(), Box::new(BuiltinMethod::new(
        "addMinutes".to_string(), 2, DixType::Timestamp,
        |args| {
            let dt  = args[0].as_datetime();
            let dur = Duration::milliseconds((args[1].as_double() * 60_000.0) as i64);
            Ok(DixValue::from_timestamp(dt + dur))
        },
        "Returns a new Timestamp offset by N minutes".to_string(),
    )));

    m.insert("addSeconds".to_string(), Box::new(BuiltinMethod::new(
        "addSeconds".to_string(), 2, DixType::Timestamp,
        |args| {
            let dt  = args[0].as_datetime();
            let dur = Duration::milliseconds((args[1].as_double() * 1_000.0) as i64);
            Ok(DixValue::from_timestamp(dt + dur))
        },
        "Returns a new Timestamp offset by N seconds".to_string(),
    )));

    m.insert("addMilliseconds".to_string(), Box::new(BuiltinMethod::new(
        "addMilliseconds".to_string(), 2, DixType::Timestamp,
        |args| {
            let dt  = args[0].as_datetime();
            let dur = Duration::milliseconds(args[1].as_long());
            Ok(DixValue::from_timestamp(dt + dur))
        },
        "Returns a new Timestamp offset by N milliseconds".to_string(),
    )));

    // ── Formatting ────────────────────────────────────────────────────────

    m.insert("format".to_string(), Box::new(BuiltinMethod::new(
        "format".to_string(), 2, DixType::String,
        |args| {
            let dt  = args[0].as_datetime();
            let fmt = args[1].as_string();
            Ok(DixValue::from_string(dt.format(&fmt).to_string()))
        },
        "Formats the Timestamp using a strftime format string → String".to_string(),
    )));

    // ── Component accessors ───────────────────────────────────────────────

    m.insert("year".to_string(), Box::new(BuiltinMethod::new(
        "year".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().year())),
        "Year component".to_string(),
    )));

    m.insert("month".to_string(), Box::new(BuiltinMethod::new(
        "month".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().month() as i32)),
        "Month component (1–12)".to_string(),
    )));

    m.insert("day".to_string(), Box::new(BuiltinMethod::new(
        "day".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().day() as i32)),
        "Day-of-month component (1–31)".to_string(),
    )));

    m.insert("hour".to_string(), Box::new(BuiltinMethod::new(
        "hour".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().hour() as i32)),
        "Hour component (0–23)".to_string(),
    )));

    m.insert("minute".to_string(), Box::new(BuiltinMethod::new(
        "minute".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().minute() as i32)),
        "Minute component (0–59)".to_string(),
    )));

    m.insert("second".to_string(), Box::new(BuiltinMethod::new(
        "second".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().second() as i32)),
        "Second component (0–59)".to_string(),
    )));

    m.insert("millisecond".to_string(), Box::new(BuiltinMethod::new(
        "millisecond".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(
            (args[0].as_datetime().timestamp_subsec_nanos() / 1_000_000) as i32
        )),
        "Millisecond component (0–999)".to_string(),
    )));

    m.insert("dayOfWeek".to_string(), Box::new(BuiltinMethod::new(
        "dayOfWeek".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(
            args[0].as_datetime().weekday().num_days_from_sunday() as i32
        )),
        "Day of week (0=Sunday … 6=Saturday)".to_string(),
    )));

    m.insert("dayOfYear".to_string(), Box::new(BuiltinMethod::new(
        "dayOfYear".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().ordinal() as i32)),
        "Day of year (1–366)".to_string(),
    )));

    // ── Unix time ─────────────────────────────────────────────────────────

    m.insert("toUnixTime".to_string(), Box::new(BuiltinMethod::new(
        "toUnixTime".to_string(), 1, DixType::Long,
        |args| Ok(DixValue::from_long(args[0].as_datetime().timestamp())),
        "Seconds since 1970-01-01 as Long".to_string(),
    )));

    m.insert("toUnixMillis".to_string(), Box::new(BuiltinMethod::new(
        "toUnixMillis".to_string(), 1, DixType::Long,
        |args| Ok(DixValue::from_long(args[0].as_datetime().timestamp_millis())),
        "Milliseconds since 1970-01-01 as Long".to_string(),
    )));

    m
}

// ═══════════════════════════════════════════════════════════════════════════
// Date instance methods
// ═══════════════════════════════════════════════════════════════════════════

pub fn get_date_methods() -> HashMap<String, Box<dyn IBuiltinMethod>> {
    let mut m: HashMap<String, Box<dyn IBuiltinMethod>> = HashMap::new();

    m.insert("format".to_string(), Box::new(BuiltinMethod::new(
        "format".to_string(), 2, DixType::String,
        |args| {
            let dt  = args[0].as_datetime();
            let fmt = args[1].as_string();
            Ok(DixValue::from_string(dt.format(&fmt).to_string()))
        },
        "Formats the Date using a strftime format string → String".to_string(),
    )));

    m.insert("addDays".to_string(), Box::new(BuiltinMethod::new(
        "addDays".to_string(), 2, DixType::Date,
        |args| {
            let dt  = args[0].as_datetime();
            let dur = Duration::milliseconds((args[1].as_double() * 86_400_000.0) as i64);
            Ok(DixValue::from_date(dt + dur))
        },
        "Returns a new Date offset by N days".to_string(),
    )));

    m.insert("year".to_string(), Box::new(BuiltinMethod::new(
        "year".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().year())),
        "Year component".to_string(),
    )));

    m.insert("month".to_string(), Box::new(BuiltinMethod::new(
        "month".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().month() as i32)),
        "Month component (1–12)".to_string(),
    )));

    m.insert("day".to_string(), Box::new(BuiltinMethod::new(
        "day".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().day() as i32)),
        "Day-of-month component (1–31)".to_string(),
    )));

    m.insert("dayOfWeek".to_string(), Box::new(BuiltinMethod::new(
        "dayOfWeek".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(
            args[0].as_datetime().weekday().num_days_from_sunday() as i32
        )),
        "Day of week (0=Sunday … 6=Saturday)".to_string(),
    )));

    m.insert("dayOfYear".to_string(), Box::new(BuiltinMethod::new(
        "dayOfYear".to_string(), 1, DixType::Int,
        |args| Ok(DixValue::from_int(args[0].as_datetime().ordinal() as i32)),
        "Day of year (1–366)".to_string(),
    )));

    m.insert("toUnixTime".to_string(), Box::new(BuiltinMethod::new(
        "toUnixTime".to_string(), 1, DixType::Long,
        |args| Ok(DixValue::from_long(args[0].as_datetime().timestamp())),
        "Seconds since 1970-01-01 as Long".to_string(),
    )));

    m
}