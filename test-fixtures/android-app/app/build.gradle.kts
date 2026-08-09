plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
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
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.7.0")
}
