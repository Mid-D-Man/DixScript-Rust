//! Tests for `Runtime::query` -- LINQ-style querying over DixScript data
//! (`DixData::query`, `DixData::query_many`, and `DixQuery`'s chained
//! filter/sort/group/aggregate operations).
//!
//! Fixture covers two distinct shapes on purpose, matching the two entry
//! points documented in `Runtime/query.rs`:
//!   - `tasks::` (a `GroupArray`) -- exercises `query("tasks")`, since the
//!     flattener stores the full `Array` at the base path for a
//!     `GroupArray` exactly like it does for a plain `Array` literal.
//!   - `servers.web1:` / `servers.db1:` / `servers.web2:` (three separate
//!     `TableProperty` entries sharing a `servers.*` shape) -- exercises
//!     `query_many("servers.*.status")`, the sibling-wildcard case that
//!     `query(path)` alone can't reach.
//!
//! Run with:
//!   cargo test --test query_tests -- --nocapture

use dixscript::Runtime::{DixData, DixLoadOptions, DixLoader, DixQuery, DixValue};

const SRC: &str = r#"
@DATA(
  app_name = "QueryTestApp"

  tasks::
  {
    name = "Backup",
    priority = 3
  },
  {
    name = "Docs",
    priority = 1
  },
  {
    name = "Audit",
    priority = 3
  },
  {
    name = "Deploy",
    priority = 2
  }

  servers.web1:
  status = "up"

  servers.db1:
  status = "down"

  servers.web2:
  status = "up"
)
"#;

fn load() -> DixData {
    let loader = DixLoader::new();
    loader
        .load_from_str(SRC, &DixLoadOptions::new())
        .expect("fixture should compile")
}

#[test]
fn query_returns_none_for_missing_or_non_array_path() {
    let data = load();
    assert!(data.query("does_not_exist").is_none(), "missing path should be None");
    assert!(
        data.query("app_name").is_none(),
        "app_name is a String, not an Array -- query() should refuse it, not panic"
    );
}

#[test]
fn query_covers_group_array_items_via_base_path() {
    // No pattern/globbing needed -- the flattener already puts the full
    // Array at "tasks" for a GroupArray, same as it would for a plain
    // `tasks = [...]` literal.
    let data = load();
    let tasks = data.query("tasks").expect("tasks should be a queryable array");
    assert_eq!(tasks.count(), 4);
}

#[test]
fn where_filters_group_array_items_by_field() {
    let data = load();
    let high_priority = data
        .query("tasks")
        .unwrap()
        .where_(|v| v.field("priority").and_then(DixValue::as_int) == Some(3));
    assert_eq!(high_priority.count(), 2);
}

#[test]
fn where_field_eq_matches_the_plain_where_equivalent() {
    let data = load();
    let a = data
        .query("tasks")
        .unwrap()
        .where_(|v| v.field("priority").and_then(DixValue::as_int) == Some(3))
        .count();
    let b = data
        .query("tasks")
        .unwrap()
        .where_field_eq("priority", &DixValue::Int(3))
        .count();
    assert_eq!(a, b);
    assert_eq!(b, 2);
}

#[test]
fn select_projects_a_field_from_each_element() {
    let data = load();
    let filtered = data
        .query("tasks")
        .unwrap()
        .where_(|v| v.field("priority").and_then(DixValue::as_int) == Some(3));
    let names: Vec<Option<&str>> = filtered.select(|v| v.field("name").and_then(DixValue::as_string));
    assert_eq!(names, vec![Some("Backup"), Some("Audit")]);
}

#[test]
fn select_field_is_a_shorthand_for_select_with_field() {
    let data = load();
    let filtered = data
        .query("tasks")
        .unwrap()
        .where_(|v| v.field("priority").and_then(DixValue::as_int) == Some(3));
    let names = filtered.select_field("name");
    assert_eq!(
        names,
        vec![
            Some(DixValue::String("Backup".to_string())),
            Some(DixValue::String("Audit".to_string())),
        ]
    );
}

#[test]
fn order_by_desc_then_take_gets_the_top_result() {
    let data = load();
    // sort_by_key / sort_by are stable -- Backup (index 0) and Audit
    // (index 2) are tied at priority 3, so Backup wins the tie by
    // appearing first in source order.
    let top = data
        .query("tasks")
        .unwrap()
        .order_by_desc(|v| v.field("priority").and_then(DixValue::as_int).unwrap_or(0))
        .take(1);
    let names: Vec<Option<&str>> = top.select(|v| v.field("name").and_then(DixValue::as_string));
    assert_eq!(names, vec![Some("Backup")]);
}

#[test]
fn order_by_ascending_sorts_the_other_direction() {
    let data = load();
    let bottom = data
        .query("tasks")
        .unwrap()
        .order_by(|v| v.field("priority").and_then(DixValue::as_int).unwrap_or(0))
        .take(1);
    let names: Vec<Option<&str>> = bottom.select(|v| v.field("name").and_then(DixValue::as_string));
    assert_eq!(names, vec![Some("Docs")], "priority 1 is the lowest");
}

#[test]
fn skip_drops_the_leading_n_elements() {
    let data = load();
    let rest = data.query("tasks").unwrap().skip(3);
    assert_eq!(rest.count(), 1);
}

#[test]
fn group_by_priority_groups_correctly_in_first_seen_order() {
    let data = load();
    let groups = data
        .query("tasks")
        .unwrap()
        .group_by(|v| v.field("priority").and_then(DixValue::as_int).unwrap_or(0));

    // First-seen distinct priorities in fixture order: 3 (Backup), 1 (Docs), 2 (Deploy).
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].0, 3);
    assert_eq!(groups[1].0, 1);
    assert_eq!(groups[2].0, 2);

    let (_, high_priority_group) = &groups[0];
    assert_eq!(high_priority_group.len(), 2);
}

#[test]
fn any_and_all_over_a_query() {
    let data = load();
    let tasks = data.query("tasks").unwrap();
    assert!(tasks.any(|v| v.field("priority").and_then(DixValue::as_int) == Some(1)));
    assert!(!tasks.all(|v| v.field("priority").and_then(DixValue::as_int) == Some(3)));
}

#[test]
fn first_last_and_nth() {
    let data = load();
    let tasks = data.query("tasks").unwrap();
    assert_eq!(tasks.first().and_then(|v| v.field("name")).and_then(DixValue::as_string), Some("Backup"));
    assert_eq!(tasks.last().and_then(|v| v.field("name")).and_then(DixValue::as_string), Some("Deploy"));
    assert_eq!(tasks.nth(1).and_then(|v| v.field("name")).and_then(DixValue::as_string), Some("Docs"));
}

#[test]
fn query_many_matches_sibling_wildcarded_paths() {
    let data = load();
    let up_count = data
        .query_many("servers.*.status")
        .where_(|v| v.as_string() == Some("up"))
        .count();
    assert_eq!(up_count, 2);
    assert_eq!(data.query_many("servers.*.status").count(), 3, "all three servers should match the wildcard");
}

#[test]
fn field_path_walks_nested_objects() {
    // field_path is on DixValue directly, independent of DixData -- build
    // a small nested value by hand to exercise it.
    let inner = data_object(&[("name", DixValue::String("Ada".to_string()))]);
    let outer = data_object(&[("owner", inner)]);
    assert_eq!(outer.field_path("owner.name").and_then(DixValue::as_string), Some("Ada"));
    assert_eq!(outer.field_path("owner.missing"), None);
    assert_eq!(outer.field_path("missing.name"), None);
}

#[test]
fn distinct_removes_duplicate_values_preserving_first_seen_order() {
    let q = DixQuery::new(vec![
        DixValue::Int(1),
        DixValue::Int(2),
        DixValue::Int(2),
        DixValue::Int(3),
        DixValue::Int(1),
    ]);
    let deduped = q.distinct();
    assert_eq!(deduped.count(), 3);
    assert_eq!(deduped.as_slice(), &[DixValue::Int(1), DixValue::Int(2), DixValue::Int(3)]);
}

#[test]
fn sum_int_sum_float_and_avg_float() {
    let data = load();
    let priorities: Vec<i32> = data
        .query("tasks")
        .unwrap()
        .select(|v| v.field("priority").and_then(DixValue::as_int).unwrap_or(0));
    assert_eq!(priorities.into_iter().sum::<i32>(), 9); // 3 + 1 + 3 + 2

    let raw = DixQuery::new(vec![DixValue::Int(2), DixValue::Int(4), DixValue::Int(6)]);
    assert_eq!(raw.sum_int(), 12);
    assert_eq!(raw.sum_float(), 12.0);
    assert_eq!(raw.avg_float(), Some(4.0));
}

#[test]
fn min_by_key_and_max_by_key() {
    let data = load();
    let tasks = data.query("tasks").unwrap();
    let cheapest = tasks
        .min_by_key(|v| v.field("priority").and_then(DixValue::as_int).unwrap_or(i32::MAX));
    let priciest = tasks
        .max_by_key(|v| v.field("priority").and_then(DixValue::as_int).unwrap_or(i32::MIN));

    assert_eq!(cheapest.and_then(|v| v.field("name")).and_then(DixValue::as_string), Some("Docs"));
    // Backup and Audit are tied at 3 -- max_by_key returns the *last*
    // maximum element (std::iter::Iterator::max_by_key's documented
    // tie-breaking rule), so Audit wins here, not Backup.
    assert_eq!(priciest.and_then(|v| v.field("name")).and_then(DixValue::as_string), Some("Audit"));
}

#[test]
fn empty_query_terminal_ops_dont_panic() {
    let empty = DixQuery::new(vec![]);
    assert!(empty.is_empty());
    assert_eq!(empty.count(), 0);
    assert_eq!(empty.first(), None);
    assert_eq!(empty.sum_int(), 0);
    assert_eq!(empty.avg_float(), None);
    assert!(!empty.any(|_| true));
    assert!(empty.all(|_| false), "vacuous truth -- matches Iterator::all's own behavior on an empty iterator");
}

// ── local helper (not part of the public API under test) ───────────────────

fn data_object(fields: &[(&str, DixValue)]) -> DixValue {
    use std::collections::HashMap;
    let mut map = HashMap::new();
    for (k, v) in fields {
        map.insert(k.to_string(), v.clone());
    }
    DixValue::Object(map)
  }
