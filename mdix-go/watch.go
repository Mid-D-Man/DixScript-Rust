// watch.go — hot reload for a Database loaded from a file.
//
// Go counterpart to C#'s Database.EnableHotReload() (see
// mdix-csharp/tests/.../MdixHotReloadTests.cs for the reference
// behavior: file-path-only, idempotent enable, OnReloaded/OnReloadFailed
// events, safe handle swap). C# gets file-change notifications for free
// from .NET's FileSystemWatcher; Go has no equivalent in the standard
// library, and mdix-go's whole design point (see go.mod) is zero runtime
// dependencies beyond cgo — pulling in fsnotify just for this one feature
// would break that. So this polls the file's mtime via os.Stat instead,
// on a plain time.Ticker. Simpler, one fewer dependency, costs a bounded
// amount of latency (one polling interval) before a change is noticed —
// a fair trade for a config-reload feature, not a real-time file monitor.
package dixscript

import (
	"os"
	"sync"
	"time"

	"github.com/Mid-D-Man/dixscript-go/internal"
)

// defaultHotReloadInterval is used when EnableHotReload is called with
// interval <= 0. Matches the order of magnitude of C#'s own
// ReloadDebounceTicks window.
const defaultHotReloadInterval = 500 * time.Millisecond

// hotReloadState holds everything an active poll needs. Kept behind a
// pointer on Database (nil until EnableHotReload is first called) so a
// Database that never uses hot reload doesn't carry this weight.
type hotReloadState struct {
	stop chan struct{}
	done chan struct{}

	mu       sync.Mutex
	lastMod  time.Time
	onReload []func(*Database)
	onFail   []func(error)
}

// EnableHotReload starts polling the file this Database was loaded from
// (via Load — see SourcePath) and reloads automatically whenever its
// mtime changes. interval controls how often it's checked; pass 0 for
// the default 500ms.
//
// On a successful reload, every OnReloaded callback fires with db
// itself — the same pointer, its internal handle swapped in place under
// lock, so anything already holding db sees the fresh data immediately
// with no re-fetch needed. On a failed reload (the file was mid-write and
// failed to parse, say), every OnReloadFailed callback fires instead and
// db keeps serving its last-good data untouched.
//
// Returns an error immediately, without starting anything, if db wasn't
// loaded via Load() (LoadStr and the encrypted-load family have no
// single on-disk file to watch) or if hot reload is already enabled —
// call DisableHotReload first to change the interval.
func (db *Database) EnableHotReload(interval time.Duration) error {
	db.mu.Lock()
	if db.closed {
		db.mu.Unlock()
		return errClosed("Database")
	}
	if db.sourcePath == "" {
		db.mu.Unlock()
		return errNative("EnableHotReload: Database was not loaded via Load() — no file to watch")
	}
	if db.hotReload != nil {
		db.mu.Unlock()
		return errNative("EnableHotReload: already enabled — call DisableHotReload first to change settings")
	}
	if interval <= 0 {
		interval = defaultHotReloadInterval
	}
	path := db.sourcePath
	var initialMod time.Time
	if fi, err := os.Stat(path); err == nil {
		initialMod = fi.ModTime()
	}
	state := &hotReloadState{
		stop:    make(chan struct{}),
		done:    make(chan struct{}),
		lastMod: initialMod,
	}
	db.hotReload = state
	db.mu.Unlock()

	go db.hotReloadLoop(state, path, interval)
	return nil
}

// DisableHotReload stops polling, if it was enabled, and blocks until the
// poll goroutine has fully exited. Safe to call multiple times and safe
// to call when hot reload was never enabled. Close() always calls this
// first, so callers don't need to remember to.
func (db *Database) DisableHotReload() {
	db.mu.Lock()
	state := db.hotReload
	db.hotReload = nil
	db.mu.Unlock()

	if state == nil {
		return
	}
	close(state.stop)
	<-state.done
}

// IsHotReloadEnabled reports whether polling is currently active.
func (db *Database) IsHotReloadEnabled() bool {
	db.mu.RLock()
	defer db.mu.RUnlock()
	return db.hotReload != nil
}

// OnReloaded registers a callback fired after every successful reload.
// Multiple callbacks may be registered and fire in registration order.
// A no-op if hot reload isn't currently enabled — call after
// EnableHotReload, not before.
func (db *Database) OnReloaded(fn func(*Database)) {
	db.mu.RLock()
	state := db.hotReload
	db.mu.RUnlock()
	if state == nil {
		return
	}
	state.mu.Lock()
	state.onReload = append(state.onReload, fn)
	state.mu.Unlock()
}

// OnReloadFailed registers a callback fired whenever a reload attempt
// fails to parse. db keeps serving its last-good data either way — this
// is purely a notification, not a fallback hook.
func (db *Database) OnReloadFailed(fn func(error)) {
	db.mu.RLock()
	state := db.hotReload
	db.mu.RUnlock()
	if state == nil {
		return
	}
	state.mu.Lock()
	state.onFail = append(state.onFail, fn)
	state.mu.Unlock()
}

func (db *Database) hotReloadLoop(state *hotReloadState, path string, interval time.Duration) {
	defer close(state.done)
	ticker := time.NewTicker(interval)
	defer ticker.Stop()

	for {
		select {
		case <-state.stop:
			return
		case <-ticker.C:
			fi, err := os.Stat(path)
			if err != nil {
				// File momentarily missing or unreadable (e.g. an editor
				// mid-rewrite deleted-then-recreated it) — try again next
				// tick rather than treating this as a reload failure.
				continue
			}

			mod := fi.ModTime()
			state.mu.Lock()
			changed := mod.After(state.lastMod)
			if changed {
				state.lastMod = mod
			}
			state.mu.Unlock()
			if !changed {
				continue
			}

			h := internal.Load(path)
			if h == nil {
				db.fireReloadFailed(state, loadError(path))
				continue
			}

			db.mu.Lock()
			if db.closed {
				// Closed while this reload was in flight — drop the new
				// handle rather than resurrecting a closed Database, and
				// stop polling; Close()'s DisableHotReload call is
				// already waiting on state.done for exactly this exit.
				db.mu.Unlock()
				internal.Free(h)
				return
			}
			old := db.handle
			db.handle = h
			db.mu.Unlock()
			internal.Free(old)

			db.fireReloaded(state)
		}
	}
}

func (db *Database) fireReloaded(state *hotReloadState) {
	state.mu.Lock()
	callbacks := append([]func(*Database){}, state.onReload...)
	state.mu.Unlock()
	for _, fn := range callbacks {
		fn(db)
	}
}

func (db *Database) fireReloadFailed(state *hotReloadState, err error) {
	state.mu.Lock()
	callbacks := append([]func(error){}, state.onFail...)
	state.mu.Unlock()
	for _, fn := range callbacks {
		fn(err)
	}
}
