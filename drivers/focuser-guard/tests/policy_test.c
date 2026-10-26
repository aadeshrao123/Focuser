#include "../shared/guard_policy.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static FG_STATE state;
static FG_REPLY reply;
static FG_MESSAGE message;
static const uint8_t installation[16] = {1, 2, 3, 4};
static uint64_t sequence;
static uint64_t clock_units(uint64_t seconds) {
    return seconds > UINT64_MAX / FG_SECOND ? UINT64_MAX - 1 : seconds * FG_SECOND;
}
static int protected_at(uint64_t seconds) { return fg_protected(&state, clock_units(seconds)); }
static int unload_at(uint64_t seconds) { return fg_may_unload(&state, clock_units(seconds)); }
static void reset(void) {
    fg_initialize(&state, installation);
    memset(&message, 0, sizeof(message));
    message.magic = FG_MAGIC; message.version = FG_VERSION;
    message.size = sizeof(message);
    memcpy(message.installation, installation, 16);
    sequence = 0;
}
static uint32_t send(uint32_t op, uint64_t revision, uint64_t deadline,
                     uint64_t utc, uint64_t ticks) {
    message.sequence = ++sequence; message.operation = op;
    message.revision = revision;
    message.until_unix_100ns = deadline == FG_INDEFINITE ? FG_INDEFINITE : clock_units(deadline);
    return fg_dispatch(&state, &message, sizeof(message), clock_units(utc), clock_units(ticks), &reply);
}
static void snapshot(uint64_t revision, uint64_t deadline, uint64_t utc, uint64_t ticks) {
    assert(send(FG_BEGIN_CHANGE, revision, 0, utc, ticks) == FG_OK);
    assert(send(FG_COMMIT_POLICY, revision, deadline, utc, ticks) == FG_OK);
}
static void boot_disconnect_and_original_expiry(void) {
    reset(); assert(protected_at(0));
    snapshot(1, 1100, 1000, 10);
    assert(protected_at(109));
    fg_new_connection(&state); sequence = 0;
    assert(protected_at(109));
    assert(send(FG_QUERY, 0, 0, 1001, 11) == FG_OK);
    assert(reply.revision == 1);
    assert(!protected_at(110));
    snapshot(1, 1100, 900, 111); /* Clock rollback + restart never renew. */
    assert(!protected_at(111));
    assert(!unload_at(111)); /* Expiry alone is not an unload permit. */
}
static void malformed_and_replay(void) {
    size_t n;
    reset();
    for (n = 0; n < sizeof(message); ++n)
        assert(fg_dispatch(&state, &message, n, 0, 0, &reply) == FG_MALFORMED);
    assert(fg_dispatch(&state, &message, sizeof(message) + 1, 0, 0, &reply) == FG_MALFORMED);
    assert(send(FG_QUERY, 0, 0, 0, 0) == FG_OK);
    assert(fg_dispatch(&state, &message, sizeof(message), 0, 0, &reply) == FG_REPLAY);
    message.installation[0] ^= 1;
    assert(send(FG_QUERY, 0, 0, 0, 0) == FG_IDENTITY);
    message.installation[0] ^= 1;
    message.reserved_tail = 1;
    assert(send(FG_QUERY, 0, 0, 0, 0) == FG_MALFORMED);
    message.reserved_tail = 0; message.version = 2;
    assert(send(FG_QUERY, 0, 0, 0, 0) == FG_MALFORMED);
}
static void revision_and_transition_races(void) {
    reset(); snapshot(10, 2000, 1000, 0);
    assert(send(FG_BEGIN_CHANGE, 9, 0, 1000, 0) == FG_REVISION);
    assert(send(FG_COMMIT_POLICY, 10, 0, 1000, 0) == FG_BUSY);
    assert(protected_at(0));
    assert(send(FG_BEGIN_CHANGE, 11, 0, 1000, 0) == FG_OK);
    fg_new_connection(&state); sequence = 0;
    assert(protected_at(100000)); /* Interrupted writes stay fenced. */
    snapshot(11, 0, 1000, 10); /* Verified unlock, not a client flag. */
    assert(!protected_at(10));
}
static void permits_and_interrupted_removal(void) {
    reset(); snapshot(1, 2000, 1000, 0);
    message.permit[0] = 1;
    assert(send(FG_PREPARE_REMOVAL, 1, 0, 1000, 0) == FG_PROTECTED);
    snapshot(2, 0, 1000, 1);
    assert(send(FG_PREPARE_REMOVAL, 2, 0, 1000, 1) == FG_OK);
    message.permit[0] = 2;
    assert(send(FG_CONSUME_REMOVAL, 2, 0, 1000, 2) == FG_PERMIT);
    message.permit[0] = 1;
    assert(send(FG_CONSUME_REMOVAL, 2, 0, 1000, 2) == FG_PERMIT);
    assert(send(FG_PREPARE_REMOVAL, 2, 0, 1000, 2) == FG_OK);
    assert(send(FG_CONSUME_REMOVAL, 2, 0, 1000, 17) == FG_PERMIT);
    assert(send(FG_PREPARE_REMOVAL, 2, 0, 1000, 18) == FG_OK);
    assert(send(FG_CONSUME_REMOVAL, 1, 0, 1000, 18) == FG_REVISION);
    assert(send(FG_CONSUME_REMOVAL, 2, 0, 1000, 18) == FG_PERMIT);
    assert(send(FG_PREPARE_REMOVAL, 2, 0, 1000, 18) == FG_OK);
    snapshot(3, 0, 1000, 18);
    assert(send(FG_CONSUME_REMOVAL, 3, 0, 1000, 18) == FG_PERMIT);
    assert(send(FG_PREPARE_REMOVAL, 3, 0, 1000, 19) == FG_OK);
    fg_new_connection(&state); sequence = 0;
    assert(send(FG_CONSUME_REMOVAL, 3, 0, 1000, 19) == FG_PERMIT);
    assert(send(FG_PREPARE_REMOVAL, 3, 0, 1000, 20) == FG_OK);
    assert(send(FG_CONSUME_REMOVAL, 3, 0, 1000, 21) == FG_OK);
    assert(unload_at(21));
    assert(send(FG_BEGIN_CHANGE, 4, 0, 1000, 22) == FG_BUSY);
    assert(!unload_at(141));
    assert(send(FG_BEGIN_CHANGE, 4, 0, 1000, 141) == FG_OK);
    assert(protected_at(141));
}
static void indefinite_commitment_and_path_boundaries(void) {
    const uint16_t root[] = {'C', ':', '\\', 'F', 'o', 'c', 'u', 's', 'e', 'r'};
    const uint16_t child[] = {'c', ':', '\\', 'f', 'o', 'c', 'u', 's', 'e', 'r', '\\', 'a'};
    const uint16_t sibling[] = {'C', ':', '\\', 'F', 'o', 'c', 'u', 's', 'e', 'r', '2'};
    reset(); snapshot(1, FG_INDEFINITE, 1000, 10);
    assert(protected_at(UINT64_MAX - 1));
    snapshot(2, 0, 1000, 11); assert(!protected_at(11));
    assert(fg_path_within(child, 12, root, 10));
    assert(!fg_path_within(sibling, 11, root, 10));
    assert(!fg_path_within(root, 9, root, 10));
}
static void fractional_deadlines_are_not_truncated_or_renewed(void) {
    uint64_t now = 1000 * FG_SECOND + 1234567;
    uint64_t monotonic = 10 * FG_SECOND + 7654321;
    uint64_t until = 1100 * FG_SECOND + 6223800;
    uint64_t expected = monotonic + until - now;
    reset();
    message.operation = FG_COMMIT_POLICY;
    message.sequence = ++sequence;
    message.revision = 1;
    message.until_unix_100ns = until;
    assert(fg_dispatch(&state, &message, sizeof(message), now, monotonic, &reply) == FG_OK);
    assert(fg_protected(&state, expected - 1));
    assert(!fg_protected(&state, expected));
    fg_new_connection(&state); sequence = 0;
    assert(send(FG_BEGIN_CHANGE, 1, 0, 900, 11) == FG_OK);
    message.operation = FG_COMMIT_POLICY;
    message.sequence = ++sequence;
    message.until_unix_100ns = until;
    assert(fg_dispatch(&state, &message, sizeof(message), now - FG_SECOND, expected, &reply) == FG_OK);
    assert(!fg_protected(&state, expected));
}
int main(void) {
    boot_disconnect_and_original_expiry(); malformed_and_replay();
    revision_and_transition_races(); permits_and_interrupted_removal();
    indefinite_commitment_and_path_boundaries();
    fractional_deadlines_are_not_truncated_or_renewed();
    puts("PASS: 6 kernel policy scenarios, framing, identity, replay, revision, precise expiry, reconnect, permits and paths");
    return 0;
}
