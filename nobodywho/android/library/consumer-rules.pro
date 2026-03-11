# NobodyWho consumer ProGuard rules.
# These are included automatically when consuming the library AAR.

# --- JNA (used by UniFFI Kotlin bindings) ---
-keep class com.sun.jna.** { *; }
-keep interface com.sun.jna.** { *; }
-keepclassmembers class * extends com.sun.jna.Structure { *; }

# --- UniFFI generated bindings ---
# UniFFI uses reflection to look up the JNA library and callback interfaces.
-keep class uniffi.** { *; }
-keep interface uniffi.** { *; }

# --- NobodyWho public API ---
-keep class com.nobodywho.** { *; }
-keep interface com.nobodywho.** { *; }
