#ifndef MKFILTER_MAPPING_H
#define MKFILTER_MAPPING_H

#include <stddef.h>
#include <stdint.h>

#include "mkfilter_protocol.h"

#ifdef __cplusplus
extern "C" {
#endif

typedef enum MKFILTER_MAPPING_STATUS
{
    MkFilterMappingSuccess = 0,
    MkFilterMappingInvalidArgument,
    MkFilterMappingLengthMismatch,
    MkFilterMappingUnsupportedVersion,
    MkFilterMappingTooManyEntries,
    MkFilterMappingInvalidFlags,
    MkFilterMappingInvalidEntry,
    MkFilterMappingEntriesNotSorted
} MKFILTER_MAPPING_STATUS;

typedef struct MKFILTER_KEY_PACKET
{
    uint16_t MakeCode;
    uint16_t Flags;
} MKFILTER_KEY_PACKET;

void
MkFilterMappingInitialize(
    MKFILTER_MAPPING_TABLE *table
    );

size_t
MkFilterMappingSerializedSize(
    uint32_t entryCount
    );

MKFILTER_MAPPING_STATUS
MkFilterMappingValidateAndCopy(
    const void *input,
    size_t inputLength,
    MKFILTER_MAPPING_TABLE *output
    );

int
MkFilterMappingTranslate(
    const MKFILTER_MAPPING_TABLE *table,
    uint16_t sourceMakeCode,
    uint16_t sourcePrefix,
    uint16_t *targetMakeCode,
    uint16_t *targetPrefix
    );

int
MkFilterMappingApplyPacket(
    const MKFILTER_MAPPING_TABLE *table,
    MKFILTER_KEY_PACKET *packet
    );

#ifdef __cplusplus
}
#endif

#endif
