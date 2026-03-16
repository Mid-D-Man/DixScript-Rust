module github.com/Mid-D-Man/dixscript-go

go 1.21

// No external runtime dependencies — cgo handles the native bridge.
// The only requirement is CGO_ENABLED=1 and the platform native lib present
// under internal/lib/<GOOS>-<GOARCH>/.
//
// Run `cargo build -p mdix-ffi` first to generate:
//   internal/include/mdix_ffi.h   ← C header for cgo
//   internal/lib/<os>-<arch>/     ← native library (copy from target/release/)
