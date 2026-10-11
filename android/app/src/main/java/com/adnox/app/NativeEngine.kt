package com.adnox.app

/**
 * Kotlin wrapper over the Rust filtering engine (libadnox.so).
 *
 * The native methods map to the JNI exports in `engine/ffi/src/lib.rs`
 * (`Java_com_adnox_app_NativeEngine_*`). Build the shared library with:
 *
 *     cd engine
 *     cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
 *         -o ../android/app/src/main/jniLibs build --release -p adnox-ffi
 *
 * See android/README.md for the full build steps.
 */
class NativeEngine private constructor(private var handle: Long) {

    /** True if [domain] (or any parent domain) should be blocked at the DNS level. */
    fun isBlockedDomain(domain: String): Boolean {
        if (handle == 0L) return false
        return nativeIsBlockedDomain(handle, domain)
    }

    /** Number of distinct blocked domains loaded. */
    fun domainCount(): Long {
        if (handle == 0L) return 0
        return nativeDomainCount(handle)
    }

    /** Release native resources. The instance must not be used afterwards. */
    fun close() {
        if (handle != 0L) {
            nativeFree(handle)
            handle = 0L
        }
    }

    companion object {
        init {
            System.loadLibrary("adnox")
        }

        /**
         * Build an engine from the combined text of one or more filter lists
         * (Adblock Plus or hosts syntax, newline separated).
         */
        fun fromRules(rules: String): NativeEngine = NativeEngine(nativeNew(rules))

        @JvmStatic private external fun nativeNew(rules: String): Long
        @JvmStatic private external fun nativeFree(handle: Long)
        @JvmStatic private external fun nativeIsBlockedDomain(handle: Long, domain: String): Boolean
        @JvmStatic private external fun nativeDomainCount(handle: Long): Long
    }
}
