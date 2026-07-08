// v6.0 c3b — root build.gradle.kts for smix-android-runner.
//
// Root project — no module-level plugins applied here; app/ holds the
// Android library + instrumentation runner.

plugins {
    id("com.android.library") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
}
