//! FFI bridge for the AdNox engine.
//!
//! Exposes [`adnox_core::Blocklist`] through:
//!   * a **C ABI** (`adnox_*` functions) — used by the Windows service/UI and any native host;
//!   * **JNI** entry points (compiled only for Android) — used by the `VpnService` app.
//!
//! The opaque handle returned by the constructor must be released with the matching `free`
//! function. All boundary functions guard against panics and null pointers.

use std::ffi::{c_char, c_int, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use adnox_core::Blocklist;

/// Opaque handle type for C callers.
pub struct AdnoxEngine(Blocklist);

unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

/// Build an engine from newline-separated filter rules (ABP or hosts syntax).
/// Returns an opaque handle, or null on error. Free it with `adnox_engine_free`.
///
/// # Safety
/// `rules` must be a valid, NUL-terminated C string or null.
#[no_mangle]
pub unsafe extern "C" fn adnox_engine_new(rules: *const c_char) -> *mut AdnoxEngine {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let text = cstr(rules).unwrap_or("");
        let bl = Blocklist::from_lists(&[text.to_string()]);
        Box::into_raw(Box::new(AdnoxEngine(bl)))
    }));
    result.unwrap_or(ptr::null_mut())
}

/// Free an engine created by `adnox_engine_new`.
///
/// # Safety
/// `engine` must have come from `adnox_engine_new` and must not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn adnox_engine_free(engine: *mut AdnoxEngine) {
    if !engine.is_null() {
        drop(Box::from_raw(engine));
    }
}

/// Returns 1 if the domain (or any parent domain) is blocked, else 0. -1 on bad arguments.
///
/// # Safety
/// `engine` must be valid; `domain` must be a valid NUL-terminated C string or null.
#[no_mangle]
pub unsafe extern "C" fn adnox_is_blocked_domain(
    engine: *const AdnoxEngine,
    domain: *const c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return -1,
        };
        match cstr(domain) {
            Some(d) => i32::from(engine.0.is_blocked_domain(d)),
            None => -1,
        }
    }));
    result.unwrap_or(-1)
}

/// Returns 1 if the full network request is blocked, else 0. -1 on bad arguments.
///
/// # Safety
/// `engine` must be valid; the string arguments must be valid NUL-terminated C strings or null.
#[no_mangle]
pub unsafe extern "C" fn adnox_is_blocked_request(
    engine: *const AdnoxEngine,
    url: *const c_char,
    source_url: *const c_char,
    request_type: *const c_char,
) -> c_int {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let engine = match engine.as_ref() {
            Some(e) => e,
            None => return -1,
        };
        match (cstr(url), cstr(source_url), cstr(request_type)) {
            (Some(u), Some(s), Some(t)) => i32::from(engine.0.is_blocked_request(u, s, t)),
            _ => -1,
        }
    }));
    result.unwrap_or(-1)
}

/// Number of distinct blocked domains loaded. -1 on bad arguments.
///
/// # Safety
/// `engine` must be valid.
#[no_mangle]
pub unsafe extern "C" fn adnox_domain_count(engine: *const AdnoxEngine) -> c_int {
    match engine.as_ref() {
        Some(e) => e.0.domain_count() as c_int,
        None => -1,
    }
}

// --------------------------------------------------------------------------------------------
// JNI bindings (Android only). Class: com.adnox.app.NativeEngine
// --------------------------------------------------------------------------------------------
#[cfg(target_os = "android")]
mod android {
    use super::*;
    use jni::objects::{JClass, JString};
    use jni::sys::{jboolean, jlong};
    use jni::JNIEnv;

    #[no_mangle]
    pub extern "system" fn Java_com_adnox_app_NativeEngine_nativeNew(
        mut env: JNIEnv,
        _class: JClass,
        rules: JString,
    ) -> jlong {
        let text: String = match env.get_string(&rules) {
            Ok(s) => s.into(),
            Err(_) => String::new(),
        };
        let bl = Blocklist::from_lists(&[text]);
        Box::into_raw(Box::new(AdnoxEngine(bl))) as jlong
    }

    #[no_mangle]
    pub extern "system" fn Java_com_adnox_app_NativeEngine_nativeFree(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) {
        if handle != 0 {
            unsafe { drop(Box::from_raw(handle as *mut AdnoxEngine)) };
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_com_adnox_app_NativeEngine_nativeIsBlockedDomain(
        mut env: JNIEnv,
        _class: JClass,
        handle: jlong,
        domain: JString,
    ) -> jboolean {
        if handle == 0 {
            return 0;
        }
        let engine = unsafe { &*(handle as *const AdnoxEngine) };
        let d: String = match env.get_string(&domain) {
            Ok(s) => s.into(),
            Err(_) => return 0,
        };
        jboolean::from(engine.0.is_blocked_domain(&d))
    }

    #[no_mangle]
    pub extern "system" fn Java_com_adnox_app_NativeEngine_nativeDomainCount(
        _env: JNIEnv,
        _class: JClass,
        handle: jlong,
    ) -> jlong {
        if handle == 0 {
            return 0;
        }
        let engine = unsafe { &*(handle as *const AdnoxEngine) };
        engine.0.domain_count() as jlong
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn c_abi_round_trip() {
        let rules = CString::new("||doubleclick.net^\n0.0.0.0 ads.tracker.com").unwrap();
        unsafe {
            let eng = adnox_engine_new(rules.as_ptr());
            assert!(!eng.is_null());

            let blocked = CString::new("ads.g.doubleclick.net").unwrap();
            assert_eq!(adnox_is_blocked_domain(eng, blocked.as_ptr()), 1);

            let allowed = CString::new("example.com").unwrap();
            assert_eq!(adnox_is_blocked_domain(eng, allowed.as_ptr()), 0);

            assert_eq!(adnox_is_blocked_domain(eng, ptr::null()), -1);
            assert!(adnox_domain_count(eng) >= 2);

            adnox_engine_free(eng);
        }
    }
}
