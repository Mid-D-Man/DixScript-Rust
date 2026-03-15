"""
MidManStudio.Mdix — DixScript (.mdix) runtime for Python.

Quick start:

    from midmanstudio.mdix import MdixDatabase, MdixBuilder, MdixError, MdixResult

    # Context manager (auto-frees)
    with MdixDatabase.load_str('@DATA( port = 8080, host = "localhost" )') as db:
        port = db.get_int("port")              # raises MdixError on failure
        host = db.get_string("host", "?")      # returns "?" if missing

    # Railway style — never raises
    result = (MdixDatabase.try_load_str(source)
              .and_then(lambda db: db.try_get_int("port"))
              .map(lambda p: p * 2)
              .unwrap_or(0))

    # Builder with two-tier ordering
    db = (MdixBuilder()
          .set_string("app_name", "MyGame")    # tier 1 flat properties first
          .set_int("port", 8080)
          .with_table_properties("server",     # tier 2 grouped after
              host="localhost", port=8080)
          .with_group_array("enemies", [
              {"name": "Goblin", "hp": 50},
              {"name": "Orc",    "hp": 100},
          ])
          .to_database())
"""

from __future__ import annotations

from ._mdix import (  # type: ignore[import]
    MdixError,
    MdixResult,
    MdixDatabase,
    MdixBuilder,
    __version__,
)

__all__ = [
    "MdixError",
    "MdixResult",
    "MdixDatabase",
    "MdixBuilder",
    "__version__",
]
```

---

**`mdix-python/python/midmanstudio/mdix/py.typed`**
```
