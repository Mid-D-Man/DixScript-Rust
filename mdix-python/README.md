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

## Requirements

- Python 3.8+
- No Rust toolchain required (pre-built wheels provided)
