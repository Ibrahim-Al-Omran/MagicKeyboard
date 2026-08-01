#ifndef MKFILTER_PROTOCOL_H
#define MKFILTER_PROTOCOL_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum
{
    MKFILTER_PROTOCOL_VERSION = 1,
    MKFILTER_MAPPING_VERSION = 1,
    MKFILTER_MAPPING_MAX_ENTRIES = 64,
    MKFILTER_SCAN_CODE_MAX = 0x7f,
    MKFILTER_PREFIX_NONE = 0x0000,
    MKFILTER_PREFIX_E0 = 0x0002,
    MKFILTER_PREFIX_E1 = 0x0004,
    MKFILTER_PREFIX_MASK = 0x0006,
    MKFILTER_MAPPING_FLAGS_NONE = 0,
    MKFILTER_CAPABILITY_BOUNDED_TABLE = 0x00000001,
    MKFILTER_CAPABILITY_PACKET_INTERCEPTION_ACTIVE = 0x00000002
};

typedef struct MKFILTER_MAPPING_HEADER
{
    uint32_t Size;
    uint32_t Version;
    uint32_t EntryCount;
    uint32_t Flags;
} MKFILTER_MAPPING_HEADER;

typedef struct MKFILTER_MAPPING_ENTRY
{
    uint16_t SourceMakeCode;
    uint16_t SourcePrefix;
    uint16_t TargetMakeCode;
    uint16_t TargetPrefix;
} MKFILTER_MAPPING_ENTRY;

typedef struct MKFILTER_MAPPING_TABLE
{
    MKFILTER_MAPPING_HEADER Header;
    MKFILTER_MAPPING_ENTRY Entries[MKFILTER_MAPPING_MAX_ENTRIES];
} MKFILTER_MAPPING_TABLE;

typedef struct MKFILTER_CAPABILITIES
{
    uint32_t Size;
    uint32_t ProtocolVersion;
    uint32_t MappingVersion;
    uint32_t MaxEntries;
    uint32_t DriverVersionMajor;
    uint32_t DriverVersionMinor;
    uint32_t DriverVersionPatch;
    uint32_t MappingGeneration;
    uint32_t Flags;
} MKFILTER_CAPABILITIES;

#if defined(__cplusplus)
static_assert(sizeof(MKFILTER_MAPPING_HEADER) == 16, "mapping header ABI");
static_assert(sizeof(MKFILTER_MAPPING_ENTRY) == 8, "mapping entry ABI");
#elif defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
_Static_assert(sizeof(MKFILTER_MAPPING_HEADER) == 16, "mapping header ABI");
_Static_assert(sizeof(MKFILTER_MAPPING_ENTRY) == 8, "mapping entry ABI");
#endif

#ifdef __cplusplus
}
#endif

#endif
