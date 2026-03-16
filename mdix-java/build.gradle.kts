import org.gradle.nativeplatform.platform.internal.DefaultNativePlatform

plugins {
    kotlin("jvm") version "1.9.23"
    `java-library`
    `maven-publish`
}

group   = "com.midmanstudio"
version = "1.0.0"

repositories {
    mavenCentral()
}

dependencies {
    // Kotlin stdlib
    implementation(kotlin("stdlib"))

    // Coroutines (optional — for the suspend functions in Extensions.kt)
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0")

    // Tests
    testImplementation(kotlin("test"))
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.2")
    testImplementation("org.assertj:assertj-core:3.25.3")
}

java {
    sourceCompatibility = JavaVersion.VERSION_11
    targetCompatibility = JavaVersion.VERSION_11
    withSourcesJar()
}

kotlin {
    jvmToolchain(11)
}

// ── Cargo build task ──────────────────────────────────────────────────────────
// Runs `cargo build -p mdix-java --release` from the repo root then copies the
// platform-native lib into the resources/native/<rid>/ directory so it gets
// bundled into the JAR by the processResources task.

val cargoLibName: String by lazy {
    val os = DefaultNativePlatform.getCurrentOperatingSystem()
    when {
        os.isLinux   -> "libmdix_java.so"
        os.isMacOsX  -> "libmdix_java.dylib"
        os.isWindows -> "mdix_java.dll"
        else         -> "libmdix_java.so"
    }
}

val nativeRid: String by lazy {
    val os   = DefaultNativePlatform.getCurrentOperatingSystem()
    val arch = DefaultNativePlatform.getCurrentArchitecture()
    val archStr = when {
        arch.isArm -> "aarch64"
        else       -> "x86_64"
    }
    when {
        os.isLinux   -> "linux-$archStr"
        os.isMacOsX  -> "darwin-$archStr"
        os.isWindows -> "win32-x86-64"
        else         -> "linux-$archStr"
    }
}

val cargoTargetDir: String by lazy {
    // Assume the repo root is one level above mdix-java/
    rootProject.projectDir.parentFile.resolve("target/release").absolutePath
}

val nativeOutputDir: File by lazy {
    layout.projectDirectory.dir("src/main/resources/native/$nativeRid").asFile
}

val buildNativeLib by tasks.registering(Exec::class) {
    description = "Compiles the Rust mdix-java crate for the current platform"
    group       = "build"

    workingDir = rootProject.projectDir.parentFile  // repo root
    commandLine("cargo", "build", "-p", "mdix-java", "--release")

    doLast {
        nativeOutputDir.mkdirs()
        val src  = File(cargoTargetDir, cargoLibName)
        val dest = File(nativeOutputDir, cargoLibName)
        if (src.exists()) {
            src.copyTo(dest, overwrite = true)
            println("Copied $src → $dest")
        } else {
            logger.warn("Native lib not found at $src — JAR will lack native support")
        }
    }
}

// Wire the native build into the standard Gradle lifecycle.
// Running `./gradlew build` compiles Rust, then Java/Kotlin, then packages everything.
tasks.named("processResources") {
    dependsOn(buildNativeLib)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

tasks.test {
    useJUnitPlatform()
    // Pass java.library.path so tests can find the native lib without a JAR.
    jvmArgs("-Djava.library.path=${nativeOutputDir.absolutePath}")
}

// ── Publishing ────────────────────────────────────────────────────────────────

publishing {
    publications {
        create<MavenPublication>("mavenJava") {
            from(components["java"])
            artifactId = "dixscript-java"

            pom {
                name.set("DixScript Java/Kotlin")
                description.set("Java and Kotlin bindings for the DixScript (.mdix) runtime")
                url.set("https://github.com/Mid-D-Man/DixScript-Rust")
                licenses {
                    license {
                        name.set("MIT")
                        url.set("https://opensource.org/licenses/MIT")
                    }
                }
            }
        }
    }
    repositories {
        // Local Maven cache — useful for testing before publishing to Maven Central.
        mavenLocal()
    }
}
