/*
 * AdNox engine — C ABI.
 *
 * Link against libadnox (cdylib/staticlib built from the `adnox-ffi` crate) and include this
 * header. All strings are UTF-8, NUL-terminated. The engine handle is opaque; create it with
 * adnox_engine_new() and release it with adnox_engine_free().
 *
 * Thread-safety: an AdnoxEngine is NOT internally synchronised and is `!Send` on the Rust side.
 * Use one engine per thread, or guard calls with your own mutex.
 */
#ifndef ADNOX_H
#define ADNOX_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct AdnoxEngine AdnoxEngine;

/* Build an engine from newline-separated filter rules (ABP or hosts syntax).
 * Returns NULL on error. */
AdnoxEngine *adnox_engine_new(const char *rules);

/* Free an engine created by adnox_engine_new(). */
void adnox_engine_free(AdnoxEngine *engine);

/* 1 = blocked, 0 = allowed, -1 = bad arguments. Matches the domain and all subdomains. */
int adnox_is_blocked_domain(const AdnoxEngine *engine, const char *domain);

/* 1 = blocked, 0 = allowed, -1 = bad arguments.
 * request_type is an Adblock resource type: "script", "image", "stylesheet",
 * "xmlhttprequest", "sub_frame", "document", "other", ... */
int adnox_is_blocked_request(const AdnoxEngine *engine,
                             const char *url,
                             const char *source_url,
                             const char *request_type);

/* Number of distinct blocked domains loaded, or -1 on bad arguments. */
int adnox_domain_count(const AdnoxEngine *engine);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* ADNOX_H */
