// The runner's app module.
//
// Android library carrying an instrumentation test runner. Pattern
// mirrors mobile UI test frameworks (Maestro/Detox): a "test" APK
// instrumented via `adb shell am instrument` that boots an embedded
// HTTP server + keeps process alive while host-side Rust driver
// dispatches over adb-forwarded port 28080.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "dev.smix.runner"
    compileSdk = 35

    defaultConfig {
        minSdk = 33
        targetSdk = 35

        // Wire instrumentation test runner from this module.
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
        named("test") {
            kotlin.srcDirs("src/test/kotlin")
        }
    }

    packaging {
        resources.excludes += setOf(
            "META-INF/AL2.0",
            "META-INF/LGPL2.1",
            "META-INF/LICENSE.md",
            "META-INF/LICENSE-notice.md",
        )
    }
}

dependencies {
    // androidx core (AndroidManifest etc.)
    implementation("androidx.core:core-ktx:1.13.1")

    // JVM unit tests for RunnerWire (pure wire logic). Real org.json on
    // the test classpath overrides the mockable android.jar stubs.
    testImplementation("junit:junit:4.13.2")
    testImplementation("org.json:json:20240303")

    // UiAutomator2 — primary sense+act backend (mirror swift XCUITest).
    androidTestImplementation("androidx.test.uiautomator:uiautomator:2.3.0")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test:rules:1.6.1")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("junit:junit:4.13.2")

    // NanoHTTPD — single-file embedded HTTP server. Mirror of swift
    // FlyingFox / Hummingbird in smix-runner. Tiny dep (~80KB), Android-
    // friendly, no dependencies of its own.
    androidTestImplementation("org.nanohttpd:nanohttpd:2.3.1")

    // JSON
    androidTestImplementation("org.json:json:20240303")

    // Text recognition, Latin script. On-device, so OCR needs no network
    // and no key — the counterpart to Apple's Vision on the iOS side.
    androidTestImplementation("com.google.mlkit:text-recognition:16.0.1")
}
