plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.smix.fixture"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.smix.fixture"
        minSdk = 33
        targetSdk = 35
        versionCode = 1
        versionName = "1.0"
    }
    // Debug only: this is never released, and a signing config would be
    // one more thing to keep in step with nothing.
    buildTypes { getByName("debug") { isMinifyEnabled = false } }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.8.0")
    // A list a filter narrows, the shape of a consumer's device search.
    implementation("androidx.recyclerview:recyclerview:1.4.0")
    // The Compose half of the fixture. A Compose text field and an
    // EditText present themselves to accessibility differently, and a
    // gate that only ever drives one cannot see a predicate that is
    // true of that one and false of the other.
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    // LazyColumn: a list whose far rows are never composed, which is one
    // of the three ways the two perception paths part company.
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.activity:activity-compose:1.13.0")
    // The probe. Debug only, and the fixture keeps `testTagsAsResourceId`
    // on alongside it: the two-paths reconciliation needs BOTH paths
    // answering at once, so removing the opt-in here would leave nothing
    // to compare against.
    debugImplementation("jp.golia.smix:smix-probe")
}

kotlin {
    compilerOptions { jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17) }
}
