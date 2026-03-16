package com.midmanstudio.dixscript.internal;

import java.io.*;
import java.nio.file.*;
import java.util.Locale;

/**
 * Extracts and loads the platform-native mdix_java library from the JAR.
 *
 * Libraries are bundled under /native/<rid>/ in the JAR resources, where
 * <rid> follows the Maven/JNA RID convention:
 *   linux-x86_64     → libmdix_java.so
 *   darwin-aarch64   → libmdix_java.dylib
 *   darwin-x86_64    → libmdix_java.dylib
 *   win32-x86-64     → mdix_java.dll
 *
 * On first load the library is extracted to a temp directory.  The temp file
 * is marked deleteOnExit() so it is cleaned up when the JVM shuts down.
 *
 * If the library cannot be found in the JAR, we fall back to
 * System.loadLibrary("mdix_java") which searches java.library.path.
 * This makes development without the JAR (e.g. running from the IDE with
 * the native lib on LD_LIBRARY_PATH) work transparently.
 */
public final class NativeLoader {

    private static volatile boolean loaded = false;
    private static final Object lock = new Object();

    /** Call once — safe to call multiple times. */
    public static void load() {
        if (loaded) return;
        synchronized (lock) {
            if (loaded) return;
            doLoad();
            loaded = true;
        }
    }

    private static void doLoad() {
        String rid   = rid();
        String libName = libName();
        String resource = "/native/" + rid + "/" + libName;

        try (InputStream is = NativeLoader.class.getResourceAsStream(resource)) {
            if (is == null) {
                // Not bundled in JAR — fall back to system library path.
                System.loadLibrary("mdix_java");
                return;
            }

            // Extract to a temp file.
            Path tmp = Files.createTempFile("mdix_java_", libSuffix());
            tmp.toFile().deleteOnExit();
            try (OutputStream os = Files.newOutputStream(tmp)) {
                byte[] buf = new byte[8192];
                int n;
                while ((n = is.read(buf)) != -1) os.write(buf, 0, n);
            }
            System.load(tmp.toAbsolutePath().toString());

        } catch (IOException e) {
            throw new UnsatisfiedLinkError(
                "Failed to extract native library " + resource + ": " + e.getMessage());
        }
    }

    /** Returns the JNA-style Runtime Identifier for the current platform. */
    static String rid() {
        String os   = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        String arch = System.getProperty("os.arch", "").toLowerCase(Locale.ROOT);

        String normalizedArch;
        if (arch.contains("aarch64") || arch.contains("arm64")) {
            normalizedArch = "aarch64";
        } else if (arch.contains("amd64") || arch.contains("x86_64") || arch.contains("x86-64")) {
            normalizedArch = "x86_64";
        } else {
            normalizedArch = arch;
        }

        if (os.contains("linux")) {
            return "linux-" + normalizedArch;
        } else if (os.contains("mac") || os.contains("darwin")) {
            return "darwin-" + normalizedArch;
        } else if (os.contains("windows")) {
            return "win32-x86-64";
        } else {
            return os.replace(' ', '_') + "-" + normalizedArch;
        }
    }

    private static String libName() {
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        if (os.contains("windows")) return "mdix_java.dll";
        if (os.contains("mac") || os.contains("darwin")) return "libmdix_java.dylib";
        return "libmdix_java.so";
    }

    private static String libSuffix() {
        String os = System.getProperty("os.name", "").toLowerCase(Locale.ROOT);
        if (os.contains("windows")) return ".dll";
        if (os.contains("mac") || os.contains("darwin")) return ".dylib";
        return ".so";
    }

    private NativeLoader() {}
    }
