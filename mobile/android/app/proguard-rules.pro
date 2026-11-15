# JNA and the UniFFI binding look classes and fields up by name (and JNA calls back into them): keep them.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class uniffi.** { *; }
-dontwarn java.awt.**
