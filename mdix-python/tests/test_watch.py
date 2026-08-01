"""Tests for MdixWatcher — poll-based hot reload.

MdixWatcher was already implemented and registered in the native
extension (`src/watch.rs`) but wasn't exported from
`midmanstudio.mdix.__init__` and had no test coverage. These tests cover
its actual behavior per `dixscript::Runtime::HotReloadWatcher`'s
contract (see `hot_reload.rs`): the first `check()` always reports a
change, later calls only reload when the file's mtime has moved.
"""

import os
import time

import pytest
from midmanstudio.mdix import MdixDatabase, MdixWatcher, MdixError


def _bump_mtime(path, seconds_ahead=2):
    """Explicitly advance a file's mtime rather than relying on real
    wall-clock delay between writes — avoids flakiness on filesystems
    with coarse (e.g. 1s) mtime resolution."""
    now = time.time()
    new_time = now + seconds_ahead
    os.utime(path, (new_time, new_time))


class TestConstruction:

    def test_path_property(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        assert str(p) in watcher.path

    def test_has_loaded_false_before_first_check(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        assert watcher.has_loaded is False

    def test_repr_contains_path(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        assert "MdixWatcher" in repr(watcher)


class TestCheck:

    def test_first_check_always_reports_changed(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))

        db, changed = watcher.check()
        assert changed is True
        assert db is not None
        assert db.get_int("port") == 8080
        assert watcher.has_loaded is True

    def test_second_check_with_no_change_reports_unchanged(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        watcher.check()

        db, changed = watcher.check()
        assert changed is False
        assert db is None

    def test_check_after_file_modified_reloads_new_content(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        watcher.check()

        p.write_text('@DATA( port = 9090 )')
        _bump_mtime(p)

        db, changed = watcher.check()
        assert changed is True
        assert db.get_int("port") == 9090

    def test_check_on_missing_file_raises(self, tmp_path):
        p = tmp_path / "does_not_exist.mdix"
        watcher = MdixWatcher(str(p))
        with pytest.raises(MdixError):
            watcher.check()


class TestForceReload:

    def test_force_reload_ignores_unchanged_state(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        watcher.check()

        # No file change in between -- force_reload should still return
        # a fresh, valid database rather than None.
        db = watcher.force_reload()
        assert db.get_int("port") == 8080

    def test_force_reload_updates_internal_stamp(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))

        watcher.force_reload()
        # Immediately after force_reload, an unchanged file should report
        # has_changed() == False, same as after a normal check().
        assert watcher.has_changed() is False


class TestHasChanged:

    def test_has_changed_true_before_any_check(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        assert watcher.has_changed() is True

    def test_has_changed_does_not_reload(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))

        watcher.has_changed()
        # has_changed() alone must not count as a load.
        assert watcher.has_loaded is False

    def test_has_changed_false_immediately_after_check(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        watcher.check()
        assert watcher.has_changed() is False

    def test_has_changed_true_after_file_modified(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( port = 8080 )')
        watcher = MdixWatcher(str(p))
        watcher.check()

        p.write_text('@DATA( port = 9090 )')
        _bump_mtime(p)

        assert watcher.has_changed() is True


class TestIntegrationWithDatabase:

    def test_reloaded_database_is_a_normal_usable_mdix_database(self, tmp_path):
        p = tmp_path / "config.mdix"
        p.write_text('@DATA( name = "App", tags:: "a", "b" )')
        watcher = MdixWatcher(str(p))

        db, _ = watcher.check()
        assert isinstance(db, MdixDatabase)
        assert db.get_string("name") == "App"
        assert db.get_array_length("tags") == 2
        db.close()
