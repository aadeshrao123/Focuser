#include "guard_policy.h"
#include <string.h>

static uint64_t add_ticks(uint64_t a, uint64_t b) {
    return UINT64_MAX - a < b ? UINT64_MAX : a + b;
}
static int nonzero(const uint8_t value[16]) {
    unsigned int i;
    for (i = 0; i < 16; ++i) if (value[i]) return 1;
    return 0;
}
void fg_initialize(FG_STATE *s, const uint8_t installation[16]) {
    memset(s, 0, sizeof(*s));
    memcpy(s->installation, installation, 16);
    /* Unknown boot state never means an empty policy. Controller must restore
       the authoritative DB before clearing this fence. Not a new commitment. */
    s->transition = 1;
}
void fg_new_connection(FG_STATE *s) {
    s->sequence = 0;
    s->has_permit = 0;
    /* Connection loss/reconnect neither renews nor releases the commitment. */
}
int fg_protected(const FG_STATE *s, uint64_t ticks) {
    return s->transition || (s->until_ticks && ticks < s->until_ticks);
}
int fg_may_unload(const FG_STATE *s, uint64_t ticks) {
    return !fg_protected(s, ticks) && s->removing && ticks < s->removal_ticks;
}
static uint32_t apply(FG_STATE *s, const FG_MESSAGE *m, uint64_t utc, uint64_t ticks) {
    if (m->magic != FG_MAGIC || m->version != FG_VERSION || m->size != sizeof(*m)
        || m->reserved || m->reserved_tail || m->operation < FG_BEGIN_CHANGE
        || m->operation > FG_CONSUME_REMOVAL) return FG_MALFORMED;
    if (memcmp(s->installation, m->installation, 16)) return FG_IDENTITY;
    if (!m->sequence || m->sequence <= s->sequence) return FG_REPLAY;
    s->sequence = m->sequence;
    if (m->operation == FG_QUERY) return FG_OK;
    if (m->revision < s->revision) {
        if (m->operation == FG_CONSUME_REMOVAL) s->has_permit = 0;
        return FG_REVISION;
    }
    if (s->removing && ticks < s->removal_ticks) return FG_BUSY;
    if (s->removing) {
        /* Interrupted removal cannot be silently turned into an unrestricted
           installation. A fresh authoritative snapshot must reconcile it. */
        s->removing = 0;
        s->transition = 1;
    }
    switch (m->operation) {
    case FG_BEGIN_CHANGE:
        s->transition = 1;
        s->has_permit = 0;
        return FG_OK;
    case FG_COMMIT_POLICY:
        if (!s->transition) return FG_BUSY;
        if (m->until_unix_100ns != s->until_unix_100ns) {
            s->until_ticks = m->until_unix_100ns == FG_INDEFINITE ? FG_INDEFINITE
                : m->until_unix_100ns <= utc ? 0
                : add_ticks(ticks, m->until_unix_100ns - utc);
        }
        /* Identical snapshots do not refill an expired monotonic deadline,
           even if the wall clock moved backwards in the meantime. */
        s->until_unix_100ns = m->until_unix_100ns;
        s->revision = m->revision;
        s->transition = 0;
        s->has_permit = 0;
        return FG_OK;
    case FG_PREPARE_REMOVAL:
        s->has_permit = 0;
        if (fg_protected(s, ticks)) return FG_PROTECTED;
        if (m->revision != s->revision || !nonzero(m->permit)) return FG_PERMIT;
        memcpy(s->permit, m->permit, 16);
        s->permit_ticks = add_ticks(ticks, 15 * FG_SECOND);
        s->has_permit = 1;
        return FG_OK;
    case FG_CONSUME_REMOVAL: {
        int valid = s->has_permit && ticks < s->permit_ticks
            && m->revision == s->revision && !memcmp(s->permit, m->permit, 16);
        s->has_permit = 0; /* Every attempt consumes it, including mismatches. */
        if (!valid) return FG_PERMIT;
        if (fg_protected(s, ticks)) return FG_PROTECTED;
        s->removing = 1;
        s->removal_ticks = add_ticks(ticks, 120 * FG_SECOND);
        return FG_OK;
    }
    default: return FG_MALFORMED;
    }
}
uint32_t fg_dispatch(FG_STATE *s, const void *input, size_t length,
                     uint64_t utc, uint64_t ticks, FG_REPLY *reply) {
    FG_MESSAGE m;
    uint32_t code = FG_MALFORMED;
    if (input && length == sizeof(m)) {
        memcpy(&m, input, sizeof(m)); /* Input need not be aligned. */
        code = apply(s, &m, utc, ticks);
    }
    memset(reply, 0, sizeof(*reply));
    reply->magic = FG_MAGIC;
    reply->version = FG_VERSION;
    reply->size = (uint16_t)sizeof(*reply);
    reply->code = code;
    reply->sequence = s->sequence;
    reply->revision = s->revision;
    reply->until_unix_100ns = s->until_unix_100ns;
    memcpy(reply->installation, s->installation, 16);
    if (fg_protected(s, ticks)) reply->flags |= FG_FLAG_PROTECTED;
    if (s->transition) reply->flags |= FG_FLAG_TRANSITION;
    if (s->removing) reply->flags |= FG_FLAG_REMOVING;
    return code;
}
static uint16_t fold(uint16_t c) { return c >= 'A' && c <= 'Z' ? c + ('a' - 'A') : c; }
int fg_path_within(const uint16_t *p, size_t n, const uint16_t *r, size_t rn) {
    size_t i;
    if (!rn || n < rn) return 0;
    for (i = 0; i < rn; ++i) if (fold(p[i]) != fold(r[i])) return 0;
    return n == rn || p[rn] == '\\' || p[rn] == ':';
}
