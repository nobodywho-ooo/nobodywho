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
# Keep public classes and their public/protected members.
-keep public class com.nobodywho.NobodyWhoModel { public protected *; }
-keep public class com.nobodywho.Chat { public protected *; }
-keep public class com.nobodywho.ChatConfig { public protected *; }
-keep public class com.nobodywho.LlamaCppEmbedder { public protected *; }
-keep public class com.nobodywho.LlamaCppReranker { public protected *; }
-keep public class com.nobodywho.LlamaCppLanguageModel { public protected *; }
-keep public class com.nobodywho.InMemoryVectorStore { public protected *; }
-keep public class com.nobodywho.SQLiteVectorStore { public protected *; }
-keep public class com.nobodywho.SemanticMemory { public protected *; }
-keep public class com.nobodywho.HybridSemanticMemory { public protected *; }
-keep public class com.nobodywho.ScoredDocument { public protected *; }
-keep public class com.nobodywho.RankedResult { public protected *; }
-keep public class com.nobodywho.PromptTemplate { public protected *; }
-keep public interface com.nobodywho.EmbedderAgent { *; }
-keep public interface com.nobodywho.VectorStore { *; }
-keep public interface com.nobodywho.Reranker { *; }
-keep public interface com.nobodywho.LanguageModel { *; }

# Protect close() and destroy() on all AutoCloseable wrappers so R8 full mode
# cannot remove them as "unused" even when called only via the interface.
-keepclassmembers class com.nobodywho.** implements java.lang.AutoCloseable {
    public void close();
}
-keepclassmembers class uniffi.** {
    public void destroy();
    public void close();
}
