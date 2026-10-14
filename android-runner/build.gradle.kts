// Root project — no module-level plugins applied here; app/ holds the
// Android library + instrumentation runner.
//
// Every plugin a subproject applies is declared here with `apply false`.
// That is not style: a plugin named with its version inside a subproject
// is loaded into *that project's* classloader scope, so two siblings
// naming the same plugin get two copies of its classes. vanniktech's
// publish plugin shares one `SonatypeRepositoryBuildService` across the
// projects it publishes, and a build service cannot cross that boundary —
// `:sdk:publish :probe:publish` in one invocation fails while either alone
// succeeds. Declaring it here loads it once, in the root scope both
// subprojects resolve against.

plugins {
    id("com.android.library") version "9.4.1" apply false
    id("org.jetbrains.kotlin.android") version "2.4.21" apply false
    id("org.jetbrains.kotlin.plugin.serialization") version "2.4.21" apply false
    id("com.vanniktech.maven.publish") version "0.37.0" apply false
}
