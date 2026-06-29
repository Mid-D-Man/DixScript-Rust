# mdix-python/python/midmanstudio/mdix/__init__.py
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

Schema validation::

    from midmanstudio.mdix import MdixSchema

    schema = (MdixSchema()
        .require_string("app_name")
        .require_int("port")
        .require_long("created_at_ms")
        .optional_bool("debug"))
    report = db.validate_schema(schema)
    if not report.is_valid:
        print(report)

Hot reload::

    from midmanstudio.mdix import MdixWatcher

    watcher = MdixWatcher("config.mdix")
    # in your update loop / tick / timer callback:
    db, changed = watcher.check()
    if changed:
        apply_new_config(db)

Merging — real AST-level merge (weighted priority, conflict reporting),
not a JSON round-trip::

    from midmanstudio.mdix import merge_files, merge_files_weighted

    db, conflicts = merge_files(["base.mdix", "patch.mdix"])
    db, conflicts = merge_files_weighted(
        [("base.mdix", 1.0), ("patch.mdix", 0.8)], strategy="weighted")

    # or merge two already-loaded databases:
    merged, conflicts = primary.merge_with(secondary, strategy="primary_wins")

Table-based serialization — the dynamic-language equivalent of this
package's reflection-based object mapping in C#::

    config = db.to_table()                       # dict / list
    db2    = MdixDatabase.from_table(config)      # round trip

ML extras (requires numpy / pandas)::

    from midmanstudio.mdix.ml import MdixNumpy, MdixMLConfig, MdixDataFrame, MdixTensor
"""

from __future__ import annotations

from ._mdix import (  # type: ignore[import]
    MdixError,
    MdixResult,
    MdixDatabase,
    MdixBuilder,
    MdixSchema,
    MdixValidationReport,
    MdixWatcher,
    merge_files,
    merge_files_weighted,
    __version__,
)

__all__ = [
    "MdixError",
    "MdixResult",
    "MdixDatabase",
    "MdixBuilder",
    "MdixSchema",
    "MdixValidationReport",
    "MdixWatcher",
    "merge_files",
    "merge_files_weighted",
    "__version__",
]
