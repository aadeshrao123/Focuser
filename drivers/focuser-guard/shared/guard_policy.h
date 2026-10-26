#ifndef FOCUSER_GUARD_POLICY_H
#define FOCUSER_GUARD_POLICY_H
#include "guard_protocol.h"
typedef struct FG_STATE {
    uint8_t installation[16], permit[16];
    uint64_t sequence, revision, until_unix_100ns, until_ticks;
    uint64_t permit_ticks, removal_ticks;
    uint32_t transition, removing, has_permit;
} FG_STATE;
/* Caller must serialize access. Both clocks use 100ns units; ticks are monotonic. */
void fg_initialize(FG_STATE *state, const uint8_t installation[16]);
void fg_new_connection(FG_STATE *state);
int fg_protected(const FG_STATE *state, uint64_t ticks);
int fg_may_unload(const FG_STATE *state, uint64_t ticks);
uint32_t fg_dispatch(FG_STATE *state, const void *input, size_t length,
                     uint64_t unix_100ns, uint64_t ticks, FG_REPLY *reply);
/* ASCII case folding is sufficient only for these fixed product path names.
 * Kernel adapter uses RtlPrefixUnicodeString/RtlEqualUnicodeString instead. */
int fg_path_within(const uint16_t *path, size_t length,
                   const uint16_t *root, size_t root_length);
#endif
