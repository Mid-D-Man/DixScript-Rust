"""Shared pytest fixtures for midmanstudio.mdix tests."""

import pytest
from midmanstudio.mdix import MdixDatabase, MdixBuilder

# ── Source constants ───────────────────────────────────────────────────────────

FLAT_SOURCE = """
@DATA(
  app_name = "TestApp"
  port     = 8080
  enabled  = true
  rate     = 1.5f
  score    = 99.9
)
"""

NESTED_SOURCE = """
@DATA(
  app_name = "Nested"
  server: host = "localhost", port = 9000, ssl = true
  db: host = "db.local", port = 5432
)
"""

ARRAY_SOURCE = """
@DATA(
  tags:: "alpha", "beta", "gamma"
  ids::  1, 2, 3
  enemies::
    { name = "Goblin", hp = 50,  ai = "AGGRESSIVE" },
    { name = "Orc",    hp = 100, ai = "AGGRESSIVE" },
    { name = "Dragon", hp = 1000, ai = "BOSS" }
)
"""

ENUMS_SOURCE = """
@ENUMS(
  LogLevel { DEBUG, INFO, WARN, ERROR }
  Status   { ACTIVE = 1, INACTIVE = 0 }
)
@DATA(
  log_level<enum> = LogLevel.INFO
  status<enum>    = Status.ACTIVE
)
"""

# Mirrors the exact mdix-scaffold regression (2026-06-29): a shallow group
# ("crates.beta", like "crates.midn-ecs") with a deeper group sharing the
# same prefix ("crates.beta.src", like "crates.midn-ecs.src"), plus an
# unrelated sibling shallow group ("crates.alpha", like "crates.midn-auth").
SCAFFOLD_LIKE_SOURCE = """
@DATA(
  project_name = "demo-core"

  crates.alpha::
    { name = "Cargo", ext = "toml", content = "" },
    { name = "lib",    ext = "rs",   content = "" }

  crates.beta::
    { name = "Cargo", ext = "toml", content = "" }

  crates.beta.src::
    { name = "lib",  ext = "rs",   content = "" },
    { name = "main", ext = "rs",   content = "" }
)
"""

# ── Fixtures ───────────────────────────────────────────────────────────────────

@pytest.fixture
def flat_db():
    db = MdixDatabase.load_str(FLAT_SOURCE)
    yield db
    db.close()


@pytest.fixture
def nested_db():
    db = MdixDatabase.load_str(NESTED_SOURCE)
    yield db
    db.close()


@pytest.fixture
def array_db():
    db = MdixDatabase.load_str(ARRAY_SOURCE)
    yield db
    db.close()


@pytest.fixture
def enums_db():
    db = MdixDatabase.load_str(ENUMS_SOURCE)
    yield db
    db.close()


@pytest.fixture
def scaffold_like_db():
    db = MdixDatabase.load_str(SCAFFOLD_LIKE_SOURCE)
    yield db
    db.close()


@pytest.fixture
def fresh_builder():
    return MdixBuilder()
