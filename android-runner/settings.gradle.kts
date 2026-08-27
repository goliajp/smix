// Mirrors the iOS runner's shape: a self-contained
// instrumentation test target that, when invoked via `adb shell am
// instrument`, starts an embedded HTTP server on port 28080 and keeps
// the process alive while serving routes (mirror /tree /find /tap
// /fill etc. — actual route impls land in c3c when fixture parity
// reached).

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

rootProject.name = "smix-android-runner"

include(":app")
include(":sdk")
include(":probe")
