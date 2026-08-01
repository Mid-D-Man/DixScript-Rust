"""
MidManStudio.Mdix — DixScript (.mdix) runtime for Python.

Quick start::

    from midmanstudio.mdix import MdixDatabase, MdixBuilder, MdixError, MdixResult

    # Context manager (auto-frees)
    with MdixDatabase.load_str('@DATA( port = 8080, host = "localhost" )') as db:
        port = db.get_int("port")
        host = db.get_string("host", "localhost")

    # Railway style — never raises
    result = (MdixDatabase.try_load_str(source)
              .and_then(lambda db: db.try_get_int("port"))
              .map(lambda p: p * 2)
              .unwrap_or(0))

    # Builder — two-tier ordering enforced
    db = (MdixBuilder()
          .set_string("app_name", "MyGame")      # tier 1 flat first
          .set_int("port", 8080)
          .with_table_properties("server",        # tier 2 grouped after
              host="localhost", port=8080)
          .with_group_array("enemies", [
              {"name": "Goblin", "hp": 50},
              {"name": "Orc",    "hp": 100},
          ])
          .to_database())

    # Merge — combine multiple .mdix sources
    db = (MdixMerger()
          .with_strategy("primary_wins")
          .merge_strings([
              ("base",    '@DATA( name = "App", port = 8080 )', 1.0),
              ("overlay", '@DATA( name = "Override", debug = true )', 0.5),
          ]))

    # Schema — validate a database against required / optional fields
    report = (MdixSchemaBuilder()
              .require_string("name")
              .require_int("port")
              .optional_bool("debug")
              .validate(db))

    if not report.is_valid:
        for err in report.errors:
            print(err)

    # Query — LINQ-style chaining over array data
    high_priority = (db.query("enemies")
                      .where_(lambda e: e["hp"] > 50)
                      .order_by_desc(lambda e: e["hp"]))
    names = high_priority.select(lambda e: e["name"])

    # query_many — sibling paths sharing shape via a wildcarded segment
    statuses = db.query_many("servers.*.status")

    # Hot reload — poll-based, call .check() from your own update loop
    watcher = MdixWatcher("config.mdix")
    reloaded, changed = watcher.check()
    if changed:
        db = reloaded

ML extras (requires numpy / pandas)::

    from midmanstudio.mdix.ml import MdixNumpy, MdixMLConfig, MdixDataFrame, MdixTensor
"""

from __future__ import annotations

from ._mdix import (  # type: ignore[import]
    MdixError,
    MdixResult,
    MdixDatabase,
    MdixBuilder,
    MdixMerger,
    MdixSchemaBuilder,
    MdixValidationReport,
    MdixValidationError,
    MdixWatcher,
    MdixQuery,
    __version__,
)

__all__ = [
    "MdixError",
    "MdixResult",
    "MdixDatabase",
    "MdixBuilder",
    "MdixMerger",
    "MdixSchemaBuilder",
    "MdixValidationReport",
    "MdixValidationError",
    "MdixWatcher",
    "MdixQuery",
    "__version__",
]
