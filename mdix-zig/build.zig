const std = @import("std");

// build.zig — mdix-zig
//
// Exposes two modules:
//   - `mdix_ffi` — raw bindings, mirrors mdix-odin's `mdix_ffi` package.
//   - `mdix`     — idiomatic wrapper (Database/Builder/HotReload so far;
//     Merge/Query/Schema/types still pending — see README.md's status
//     table), mirrors mdix-odin's `mdix` package. Its root file
//     (mdix/mdix.zig) re-exports watch.zig's HotReload at its own top
//     level, so `@import("mdix")` from outside this package sees
//     `mdix.HotReload` directly the same way Odin's flat per-directory
//     package namespace does automatically.
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

    // ── mdix — idiomatic wrapper, built on mdix_ffi ─────────────────────
    const mdix_mod = b.addModule("mdix", .{
        .root_source_file = b.path("mdix/mdix.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "mdix_ffi", .module = mdix_ffi_mod },
        },
    });
    linkMdixFfi(mdix_mod, mdix_lib_path);

    // ── tests: `zig build test` ─────────────────────────────────────────
    // Two separate test binaries, run back to back — mirrors odin-ci.yml
    // testing mdix_ffi and mdix as separate Odin packages. mdix_ffi's
    // own link-level smoke tests live in mdix_ffi.zig itself; mdix's
    // (Database/Builder/HotReload, plus watch.zig's — pulled in
    // transitively through mdix.zig's `@import("watch.zig")`) live
    // alongside the code they cover, matching mdix-c/mdix-odin style
    // rather than mdix-odin/mdix/tests/'s separate-directory one (no
    // Zig equivalent of Odin's `-define:ODIN_TEST_JSON_REPORT=` needed
    // here — see scripts/parse_zig_test_results.py's parsing approach
    // instead).
    const ffi_tests = b.addTest(.{ .root_module = mdix_ffi_mod });
    const run_ffi_tests = b.addRunArtifact(ffi_tests);

    const mdix_tests = b.addTest(.{ .root_module = mdix_mod });
    const run_mdix_tests = b.addRunArtifact(mdix_tests);

    // Separate named steps too (`test-ffi`, `test-mdix`), so CI can
    // invoke and log-capture each suite independently for its report —
    // `zig build test` alone interleaves both into one combined stream,
    // which loses the per-suite split scripts/parse_zig_test_results.py
    // wants (mirrors odin-ci.yml's per-package breakdown).
    const test_ffi_step = b.step("test-ffi", "Run only the mdix_ffi test suite");
    test_ffi_step.dependOn(&run_ffi_tests.step);
    const test_mdix_step = b.step("test-mdix", "Run only the mdix test suite");
    test_mdix_step.dependOn(&run_mdix_tests.step);

    const test_step = b.step("test", "Run the full mdix-zig test suite (needs libmdix_ffi — see -Dmdix-lib-path)");
    test_step.dependOn(&run_ffi_tests.step);
    test_step.dependOn(&run_mdix_tests.step);

    // ── examples ─────────────────────────────────────────────────────────
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
    const run_step = b.step("run-hello", "Run the raw-layer hello example (needs libmdix_ffi — see -Dmdix-lib-path)");
    run_step.dependOn(&run_hello.step);

    // `zig build run-hello-mdix` — the idiomatic-layer sibling.
    const hello_mdix_mod = b.createModule(.{
        .root_source_file = b.path("examples/hello_mdix.zig"),
        .target = target,
        .optimize = optimize,
        .imports = &.{
            .{ .name = "mdix", .module = mdix_mod },
        },
    });
    linkMdixFfi(hello_mdix_mod, mdix_lib_path);

    const hello_mdix_exe = b.addExecutable(.{
        .name = "hello_mdix",
        .root_module = hello_mdix_mod,
    });
    b.installArtifact(hello_mdix_exe);

    const run_hello_mdix = b.addRunArtifact(hello_mdix_exe);
    const run_mdix_step = b.step("run-hello-mdix", "Run the idiomatic-layer hello example (needs libmdix_ffi — see -Dmdix-lib-path)");
    run_mdix_step.dependOn(&run_hello_mdix.step);
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
