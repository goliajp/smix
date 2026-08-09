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
rootProject.name = "smix-android-fixture"
include(":app")
