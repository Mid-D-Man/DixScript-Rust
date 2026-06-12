plugins {
    id("java")
    id("org.jetbrains.kotlin.jvm") version "1.9.25"
    id("org.jetbrains.intellij")   version "1.17.4"
}

group   = providers.gradleProperty("pluginGroup").get()
version = providers.gradleProperty("pluginVersion").get()

repositories {
    mavenCentral()
}

intellij {
    // "IC" = IntelliJ Community — compatible with Rider, IDEA, Rust Rover, etc.
    type.set("IC")
    version.set(providers.gradleProperty("platformVersion").get())
    plugins.set(listOf(
        "com.redhat.devtools.lsp4ij:${providers.gradleProperty("lsp4ijVersion").get()}"
    ))
}

tasks {
    withType<JavaCompile> {
        sourceCompatibility = "17"
        targetCompatibility = "17"
    }
    withType<org.jetbrains.kotlin.gradle.tasks.KotlinCompile> {
        kotlinOptions.jvmTarget = "17"
    }
    patchPluginXml {
        sinceBuild.set(providers.gradleProperty("pluginSinceBuild").get())
        untilBuild.set("")  // open-ended — no max version
    }
processResources {
    from("src/main/resources")
}
}
