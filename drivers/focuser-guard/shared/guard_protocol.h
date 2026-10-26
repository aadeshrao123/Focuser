/* Shared, fixed-width ABI. No pointers, strings, caller identity or UI flags. */
#ifndef FOCUSER_GUARD_PROTOCOL_H
#define FOCUSER_GUARD_PROTOCOL_H
#include <stdint.h>
#include <stddef.h>
#define FG_MAGIC 0x46434744u
#define FG_VERSION 1u
#define FG_PORT_NAME L"\\FocuserGuard.v1"
#define FG_BEGIN_CHANGE 1u
#define FG_COMMIT_POLICY 2u
#define FG_QUERY 3u
#define FG_PREPARE_REMOVAL 4u
#define FG_CONSUME_REMOVAL 5u
#define FG_OK 0u
#define FG_MALFORMED 1u
#define FG_IDENTITY 2u
#define FG_REPLAY 3u
#define FG_REVISION 4u
#define FG_PROTECTED 5u
#define FG_PERMIT 6u
#define FG_BUSY 7u
#define FG_FLAG_PROTECTED 1u
#define FG_FLAG_TRANSITION 2u
#define FG_FLAG_REMOVING 4u
#define FG_INDEFINITE UINT64_MAX
#define FG_SECOND 10000000ULL
typedef struct FG_MESSAGE {
    uint32_t magic;
    uint16_t version, size;
    uint32_t operation, reserved;
    uint64_t sequence, revision, until_unix_100ns;
    uint8_t installation[16], permit[16];
    uint64_t reserved_tail;
} FG_MESSAGE;
typedef struct FG_REPLY {
    uint32_t magic;
    uint16_t version, size;
    uint32_t code, flags;
    uint64_t sequence, revision, until_unix_100ns;
    uint8_t installation[16];
} FG_REPLY;
typedef char fg_message_size_must_be_80[sizeof(FG_MESSAGE) == 80 ? 1 : -1];
typedef char fg_reply_size_must_be_56[sizeof(FG_REPLY) == 56 ? 1 : -1];
typedef char fg_installation_offset_must_be_40[offsetof(FG_MESSAGE, installation) == 40 ? 1 : -1];
typedef char fg_permit_offset_must_be_56[offsetof(FG_MESSAGE, permit) == 56 ? 1 : -1];
typedef char fg_reply_identity_offset_must_be_40[offsetof(FG_REPLY, installation) == 40 ? 1 : -1];
#endif
