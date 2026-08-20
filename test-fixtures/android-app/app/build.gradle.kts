plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.smix.fixture"
    compileSdk = 35

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
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.0")
    // The Compose half of the fixture. A Compose text field and an
    // EditText present themselves to accessibility differently, and a
    // gate that only ever drives one cannot see a predicate that is
    // true of that one and false of the other.
    implementation(platform("androidx.compose:compose-bom:2025.08.00"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.9.3")
}
