// smix-android-sdk module.
//
// Android library wrapping libsmix_ffi.so (Rust core via UniFFI 0.29.5).
// Mirrors swift-bridge/Sources/SmixCoreFFIBindings/ for iOS:
//   - JNI libs (jniLibs/{arm64-v8a,x86_64}/libsmix_ffi.so) ← rebuilt
//     by scripts/sdk/build-android-aar.sh (idempotent reproducer,
//     calls `cargo ndk -t arm64-v8a -t x86_64 -o <jniLibs> build`)
//   - Kotlin bindings (kotlin/uniffi/smix/smix.kt) ← UniFFI-generated
//     wrapper around JNA-loaded native fns
//
// Consumer (test author / RN bridge):
//   implementation project(":sdk")
//   import uniffi.smix.resolveSelector
//   resolveSelector(treeJson, selectorJson) — returns List<String>
//
// This module is intended for test targets only (instrumentation /
// dev-client) — it should not ship in an app's release build. Consumers
// wire it as `androidTestImplementation` or under a
// `debugImplementation` gate.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    // kotlinx.serialization for JSON wire encoding of Selector / A11yNode
    // (matches the Rust smix-selector wire shape via custom serializers).
    kotlin("plugin.serialization") version "2.0.21"
    // vanniktech maven-publish plugin. Sonatype Central Portal via
    // vanniktech is the modern path (OSSRH EOL 2025). The plugin
    // auto-handles the javadoc + sources + dokka jars + Central Portal
    // upload.
    id("com.vanniktech.maven.publish") version "0.30.0"
}

android {
    namespace = "dev.smix.sdk"
    compileSdk = 35

    defaultConfig {
        minSdk = 33
        targetSdk = 35

        // Wire instrumentation test runner.
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    testOptions {
        targetSdk = 35
    }

    sourceSets {
        named("main") {
            jniLibs.srcDirs("src/main/jniLibs")
            kotlin.srcDirs("src/main/kotlin")
        }
        named("test") {
            kotlin.srcDirs("src/test/kotlin")
        }
        named("androidTest") {
            kotlin.srcDirs("src/androidTest/kotlin")
        }
    }
}

dependencies {
    // UniFFI Kotlin runtime depends on JNA for native fn loading.
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.1")
    // v7.4 c1 — kotlinx.serialization-json for Selector / A11yNode
    // wire encoding (matches Rust smix-selector untagged shape).
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")

    // JVM unit test deps. MVP shape tests run on JVM (no emulator
    // needed) — Selector shape verifications and similar.
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")

    // Instrumentation test deps (mirror app module's spec).
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test:rules:1.6.1")
}

// Maven Central publish config (vanniktech 0.30.0).
//
// Namespace: jp.golia.smix (reverse DNS of smix.golia.jp).
//
// Publishing requires the following one-time setup:
//   1. DNS TXT record on smix.golia.jp with Sonatype verification string
//   2. Sonatype Central Portal account + namespace claim
//   3. GPG signing key + keyserver upload
//   4. CI secrets in GitHub repo:
//        ORG_GRADLE_PROJECT_signingInMemoryKey
//        ORG_GRADLE_PROJECT_signingInMemoryKeyPassword
//        mavenCentralUsername  (user token from central.sonatype.com)
//        mavenCentralPassword  (user token password)
//
// Publish command:
//   cd android-runner && \
//     ./gradlew :sdk:publishReleasePublicationToMavenCentralRepository

val mavenCentralGroupId = "jp.golia.smix" // reverse DNS of smix.golia.jp
val mavenCentralArtifactId = "smix-sdk"
val mavenCentralVersion = "1.0.15"

mavenPublishing {
    publishToMavenCentral(com.vanniktech.maven.publish.SonatypeHost.CENTRAL_PORTAL, automaticRelease = true)
    signAllPublications()
    coordinates(mavenCentralGroupId, mavenCentralArtifactId, mavenCentralVersion)
    pom {
        name.set("smix-sdk")
        description.set("smix iOS Simulator / Android emulator UI automation SDK — Playwright-shape Kotlin SDK")
        url.set("https://github.com/goliajp/smix")
        licenses {
            license {
                name.set("Apache-2.0 OR MIT")
                distribution.set("repo")
            }
        }
        developers {
            developer {
                id.set("doracawl")
                name.set("GOLIA K.K.")
                email.set("lihao@golia.jp")
            }
        }
        scm {
            url.set("https://github.com/goliajp/smix")
            connection.set("scm:git:git://github.com/goliajp/smix.git")
            developerConnection.set("scm:git:ssh://git@github.com/goliajp/smix.git")
        }
    }
}
