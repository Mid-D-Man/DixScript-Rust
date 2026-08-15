package mdix

// watch.odin — hot reload for a Database loaded from a file.
//
// Deliberately NOT a background thread, unlike the Go binding's
// EnableHotReload (which spins up a goroutine polling on a ticker). This
// package's usual consumer is a program that already has its own
// per-frame update loop (a game, an editor, a renderer) — spinning up a
// second thread and a mutex to protect Database.handle from it would add
// real complexity for something a one-line call already covers:
//
//   hr: mdix.Hot_Reload
//   mdix.hot_reload_init(&hr, "config.mdix")
//   defer mdix.hot_reload_destroy(&hr)
//
//   for /* your main loop */ {
//       if mdix.hot_reload_check(&hr, &db) {
//           fmt.println("config reloaded")
//       }
//       // ... rest of frame
//   }
//
// hot_reload_check does the mtime stat + conditional reload + in-place
// handle swap every call — call it as often as you'd poll anything else
// in your loop (once a frame is fine; it's a stat() call, not a full
// reparse, unless the file actually changed).

import "core:os"
import "core:strings"
import "core:time"
import ffi "../mdix_ffi"

Hot_Reload :: struct {
	path:     string, // owned clone — see hot_reload_init
	last_mod: time.Time,
	valid:    bool, // false until hot_reload_init succeeds
}

// hot_reload_init records path (cloned — the caller's string doesn't
// need to outlive this call) and its current mtime. Returns false if
// path doesn't exist or can't be stat'd; check last_error()-style via
// the returned bool only, os.Error detail is not surfaced here since
// hot_reload_check re-stats on every call anyway and will report the
// same failure there.
hot_reload_init :: proc(hr: ^Hot_Reload, path: string, allocator := context.allocator) -> bool {
	mod, err := os.modification_time_by_path(path)
	if err != nil {
		return false
	}
	hr.path = strings.clone(path, allocator)
	hr.last_mod = mod
	hr.valid = true
	return true
}

hot_reload_destroy :: proc(hr: ^Hot_Reload) {
	if hr.valid {
		delete(hr.path)
		hr.valid = false
	}
}

// hot_reload_check stats hr's file; if its mtime has advanced since the
// last check, reloads it and swaps db's handle in place (freeing the old
// one) so every other reference to the same Database sees fresh data
// with no re-fetch needed. Returns true only on an actual successful
// reload — false on "nothing changed" and on "changed but failed to
// reload" alike; check last_error() to tell those apart when it matters
// (e.g. the file was mid-write and failed to parse — db keeps serving
// its last-good data either way).
//
// A no-op returning false if hr wasn't initialized (hot_reload_init
// failed or was never called) or db.handle is nil.
hot_reload_check :: proc(hr: ^Hot_Reload, db: ^Database) -> (reloaded: bool) {
	if !hr.valid || db.handle == nil {
		return false
	}

	mod, err := os.modification_time_by_path(hr.path)
	if err != nil {
		// File momentarily missing/unreadable (e.g. an editor mid-rewrite
		// deleted-then-recreated it) — try again next check rather than
		// treating this as a reload failure.
		return false
	}
	if time.diff(hr.last_mod, mod) <= 0 {
		return false
	}
	hr.last_mod = mod

	cpath := strings.clone_to_cstring(hr.path, context.temp_allocator)
	new_handle := ffi.mdix_load(cpath)
	if new_handle == nil {
		return false
	}

	ffi.mdix_free(db.handle)
	db.handle = new_handle
	return true
}
