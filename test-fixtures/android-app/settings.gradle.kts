// A third-party-shaped app for the Android gates to drive.
//
// Every Android e2e drove Settings, a system app, and a defect that
// only affects an ordinary app's window was invisible to all of them:
// a consumer reported /tree returning the SystemUI windows and not
// theirs. A fixture that is an ordinary app is the instrument that was
// missing.
//
// Deliberately outside android-runner/ — that tree ships inside smix,
// and this must not.
pluginManagement {
    repositories {
        gradlePluginPortal()
        google()
        mavenCentral()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
// The probe comes in the way a consumer's app takes it — a debug-only
// dependency on the published coordinate — with a composite build standing
// in for the registry so the fixture exercises the real wiring without a
// publish round-trip.
includeBuild("../../android-runner") {
    // Substitution rather than renaming the gradle module: the coordinate
    // in app/build.gradle.kts stays the one a real consumer writes, and
    // only this line knows it is being served from a sibling checkout.
    dependencySubstitution {
        substitute(module("jp.golia.smix:smix-probe")).using(project(":probe"))
    }
}

rootProject.name = "smix-android-fixture"
include(":app")
