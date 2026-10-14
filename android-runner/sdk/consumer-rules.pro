# UniFFI 0.32.2 Kotlin bindings use JNA to load native methods.
# JNA's mapping requires Native methods to NOT be obfuscated.
# This file is consumed by app projects that include this library.

-keep class uniffi.** { *; }
-keep class com.sun.jna.** { *; }
-keepclassmembers class * {
    native <methods>;
}
