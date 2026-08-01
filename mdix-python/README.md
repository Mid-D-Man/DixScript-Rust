# midmanstudio-mdix

Python runtime for DixScript (`.mdix`) — MidManStudio.

## Installation
```bash
pip install midmanstudio-mdix
```

## Quick start
```python
from midmanstudio.mdix import MdixDatabase, MdixBuilder, MdixError, MdixResult

# Load and read — raises on error
with MdixDatabase.load_str('@DATA( port = 8080, host = "localhost" )') as db:
    port = db.get_int("port")
    host = db.get_string("host", "localhost")   # default if missing
    keys = db.get_keys()

# Railway style — never raises
result = (MdixDatabase.try_load_str(source)
          .and_then(lambda db: db.try_get_int("port"))
          .ensure(lambda p: p > 1024, "port must be > 1024")
          .map(lambda p: p * 2)
          .unwrap_or(3000))

# Builder — two-tier ordering enforced
db = (MdixBuilder()
      .set_config("version", "1.0.0")
      .add_enum("LogLevel", ["DEBUG", "INFO", "WARN", "ERROR"])
      # tier 1: flat properties must come first
      .set_string("app_name", "MyGame")
      .set_int("port", 8080)
      .set_bool("ssl", True)
      .set_enum("log_level", "LogLevel", "INFO")
      # tier 2: grouped after all flat
      .with_table_properties("server", {"host": "localhost", "port": 8080})
      .with_group_array("enemies", [
          {"name": "Goblin", "hp": 50},
          {"name": "Orc",    "hp": 100},
      ])
      .to_database())

# Foreign format import
db2 = MdixDatabase.from_json('{"port": 8080, "host": "localhost"}')
db3 = MdixDatabase.from_toml('port = 8080\nhost = "localhost"\n')

# Export
json_str = db.to_json(indented=True)
toml_str = db.to_toml()
mdix_str = db.to_mdix()
```

## MdixResult — railway programming
```python
# Chain operations without try/except
value = (MdixDatabase.try_load("config.mdix")
         .and_then(lambda db: db.try_get_string("server.host"))
         .map(str.upper)
         .tap(lambda v: print(f"host = {v}"))
         .unwrap_or("UNKNOWN"))

# fold — explicit success/failure branches
message = result.fold(
    on_success=lambda v: f"Loaded: {v}",
    on_failure=lambda e: f"Failed: {e}",
)

# bool(result) is True for success
if result:
    print(result.value)
```

## MdixQuery — LINQ-style querying
```python
db = MdixDatabase.load_str("""
@DATA(
  tasks::
  { name = "Backup", priority = 3 },
  { name = "Docs",   priority = 1 },
  { name = "Audit",  priority = 3 }
)
""")

high_priority = (db.query("tasks")
                  .where_(lambda t: t["priority"] == 3)
                  .order_by_desc(lambda t: t["priority"]))
names = high_priority.select(lambda t: t["name"])   # ["Backup", "Audit"]

# Sibling paths sharing shape via a wildcarded segment
statuses = db.query_many("servers.*.status")

# MdixQuery also supports len(), indexing, and iteration directly
for task in db.query("tasks"):
    print(task["name"])
```
`db.query(path)` covers a plain array literal or a GroupArray's items
alike, and returns `None` (not an error) if `path` doesn't exist or isn't
an array. Every predicate/key/selector is a plain Python callable —
`where_`, `where_field_eq`, `select`, `select_field`, `order_by`,
`order_by_desc`, `group_by`, `distinct`, `skip`, `take`, `any`, `all`,
`count`, `is_empty`, `first`/`first_or`/`last`/`nth`, `sum_int`/`sum_float`/
`avg_float`, `min_by_key`/`max_by_key`, and `to_list`.

## MdixWatcher — hot reload
```python
from midmanstudio.mdix import MdixWatcher

watcher = MdixWatcher("config.mdix")

# in your own update loop / tick / timer callback:
db, changed = watcher.check()
if changed:
    apply_new_config(db)
```
Poll-based, not OS-event-based — a single stat call per `check()`, cheap
enough to run every frame and consistent across every platform. The
first `check()` always reports a change; `db` is `None` when `changed`
is `False`. Use `force_reload()` to reload unconditionally, or
`has_changed()` to check without reloading.

## Requirements

- Python 3.8+
- No Rust toolchain required (pre-built wheels provided)
