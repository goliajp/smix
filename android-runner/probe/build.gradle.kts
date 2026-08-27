// smix-probe — the optional in-process probe.
//
// smix perceives through the accessibility tree, which is a lossy,
// asynchronous projection of what the UI toolkit knows. For a Compose app
// that projection is a bad one: the whole UI is a single
// `AndroidComposeView` and the nodes are synthesised from semantics, so a
// `testTag` only arrives at all when the app opted in with
// `testTagsAsResourceId`, dialogs host in a separate window that the opt-in
// does not reach, and a masked field's text reads back as bullets.
//
// This module is how an app hands smix the semantics tree instead. It is
// `debugImplementation` — never in a release build — and it is optional:
// without it everything works exactly as before, and `/tree` says so
// rather than quietly answering worse.
plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

group = "jp.golia.smix"
version = providers.gradleProperty("smixVersion").get()

android {
    namespace = "dev.smix.probe"
    compileSdk = 35
    defaultConfig { minSdk = 33 }
    kotlinOptions { jvmTarget = "17" }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // compileOnly, and no BOM: the app already has Compose on its
    // classpath and the probe must not drag a second copy in — nor pull
    // the BOM's constraints into a module that links against nothing.
    // The floor is the version that first published the API this reads.
    compileOnly("androidx.compose.ui:ui:1.9.3")
    testImplementation("junit:junit:4.13.2")
}
