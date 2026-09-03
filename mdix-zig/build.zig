const std = @import("std");

// build.zig — mdix-zig
//
// Exposes the `mdix_ffi` module (raw bindings, mirrors mdix-odin's
// `mdix_ffi` package) for consumers to `.addImport("mdix_ffi", ...)`.
// The idiomatic `mdix` module (Database/Builder/Watcher/Merge/Query/
// Schema, mirroring mdix-odin's `mdix` package) lands in a follow-up
// pass — see mdix-zig/README.md's status table.
//
// libmdix_ffi itself is NOT built by this script — same division of
// responsibility as mdix-c, mdix-go, and mdix-odin: build it once via
// `cargo build --release -p mdix-ffi` from the repo root, then point
// this build at the output directory.

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const mdix_lib_path = b.option(
        []const u8,
        "mdix-lib-path",
        "Directory containing the built libmdix_ffi (.so / .dylib / .lib+.dll) " ++
            "— e.g. target/release from a `cargo build --release -p mdix-ffi` " ++
            "run at the repo root. Omit to rely on the system library search " ++
            "path instead (library already installed / on LD_LIBRARY_PATH).",
    );

    // ── mdix_ffi — raw foreign bindings ─────────────────────────────────
    const mdix_ffi_mod = b.addModule("mdix_ffi", .{
        .root_source_file = b.path("mdix_ffi/mdix_ffi.zig"),
        .target = target,
        .optimize = optimize,
    });
    linkMdixFfi(mdix_ffi_mod, mdix_lib_path);

    // ── tests: `zig build test` ─────────────────────────────────────────
    // Link-level smoke tests live directly in mdix_ffi.zig (see its
    // bottom `test` blocks) since they exercise the extern declarations
    // themselves rather than idiomatic wrapper behavior.
    const ffi_tests = b.addTest(.{ .root_module = mdix_ffi_mod });
    const run_ffi_tests = b.addRunArtifact(ffi_tests);
    const test_step = b.step("test", "Run the mdix-zig test suite (needs libmdix_ffi — see -Dmdix-lib-path)");
    test_step.dependOn(&run_ffi_tests.step);

    // ── examples: `zig build run-hello` ─────────────────────────────────
    const hello_mod = b.createModule(.{
        .root_source_file = b.path("examples/hello.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "mdix_ffi", .module = mdix_ffi_mod },
        },
    });
    linkMdixFfi(hello_mod, mdix_lib_path);

    const hello_exe = b.addExecutable(.{
        .name = "hello",
        .root_module = hello_mod,
    });
    b.installArtifact(hello_exe);

    const run_hello = b.addRunArtifact(hello_exe);
    const run_step = b.step("run-hello", "Run the hello example (needs libmdix_ffi — see -Dmdix-lib-path)");
    run_step.dependOn(&run_hello.step);
}

/// Links `module` against the platform build of libmdix_ffi. If
/// `lib_path` is set, adds it as both a library search path (link time)
/// and an rpath (Linux/macOS run time) so the produced binary finds the
/// library without an `LD_LIBRARY_PATH`/`DYLD_LIBRARY_PATH` export —
/// same convention mdix-odin's README documents via
/// `-extra-linker-flags:"-L... -Wl,-rpath,..."`. Windows still needs
/// `mdix_ffi.dll` on `PATH` or next to the executable at run time (import
/// libraries don't carry rpath-equivalent metadata) — see mdix-c/README.md's
/// platform notes, which this mirrors.
fn linkMdixFfi(module: *std.Build.Module, lib_path: ?[]const u8) void {
    module.linkSystemLibrary("mdix_ffi", .{});
    if (lib_path) |p| {
        module.addLibraryPath(.{ .cwd_relative = p });
        module.addRPath(.{ .cwd_relative = p });
    }
}
