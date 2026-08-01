#include "mkfilter_mapping.h"

#include <string.h>

static int
MkFilterMappingPrefixIsValid(
    uint16_t prefix
    )
{
    return prefix == MKFILTER_PREFIX_NONE ||
           prefix == MKFILTER_PREFIX_E0 ||
           prefix == MKFILTER_PREFIX_E1;
}

static uint32_t
MkFilterMappingSourceKey(
    uint16_t makeCode,
    uint16_t prefix
    )
{
    return ((uint32_t)makeCode << 16) | (uint32_t)prefix;
}

void
MkFilterMappingInitialize(
    MKFILTER_MAPPING_TABLE *table
    )
{
    if (table == NULL)
    {
        return;
    }

    (void)memset(table, 0, sizeof(*table));
    table->Header.Size = (uint32_t)sizeof(MKFILTER_MAPPING_HEADER);
    table->Header.Version = MKFILTER_MAPPING_VERSION;
    table->Header.EntryCount = 0;
    table->Header.Flags = MKFILTER_MAPPING_FLAGS_NONE;
}

size_t
MkFilterMappingSerializedSize(
    uint32_t entryCount
    )
{
    if (entryCount > (uint32_t)MKFILTER_MAPPING_MAX_ENTRIES)
    {
        return 0;
    }

    return sizeof(MKFILTER_MAPPING_HEADER) +
           ((size_t)entryCount * sizeof(MKFILTER_MAPPING_ENTRY));
}

MKFILTER_MAPPING_STATUS
MkFilterMappingValidateAndCopy(
    const void *input,
    size_t inputLength,
    MKFILTER_MAPPING_TABLE *output
    )
{
    const unsigned char *bytes;
    MKFILTER_MAPPING_HEADER header;
    MKFILTER_MAPPING_TABLE candidate;
    size_t expectedLength;
    uint32_t index;
    uint32_t previousKey;

    if (input == NULL || output == NULL)
    {
        return MkFilterMappingInvalidArgument;
    }

    if (inputLength < sizeof(header))
    {
        return MkFilterMappingLengthMismatch;
    }

    bytes = (const unsigned char *)input;
    (void)memcpy(&header, bytes, sizeof(header));

    if (header.Version != (uint32_t)MKFILTER_MAPPING_VERSION)
    {
        return MkFilterMappingUnsupportedVersion;
    }

    if (header.EntryCount > (uint32_t)MKFILTER_MAPPING_MAX_ENTRIES)
    {
        return MkFilterMappingTooManyEntries;
    }

    if (header.Flags != (uint32_t)MKFILTER_MAPPING_FLAGS_NONE)
    {
        return MkFilterMappingInvalidFlags;
    }

    expectedLength = MkFilterMappingSerializedSize(header.EntryCount);
    if (expectedLength == 0 ||
        header.Size != expectedLength ||
        inputLength != expectedLength)
    {
        return MkFilterMappingLengthMismatch;
    }

    MkFilterMappingInitialize(&candidate);
    candidate.Header = header;
    previousKey = 0;

    for (index = 0; index < header.EntryCount; ++index)
    {
        MKFILTER_MAPPING_ENTRY entry;
        uint32_t sourceKey;
        size_t offset;

        offset = sizeof(MKFILTER_MAPPING_HEADER) +
                 ((size_t)index * sizeof(MKFILTER_MAPPING_ENTRY));
        (void)memcpy(&entry, bytes + offset, sizeof(entry));

        if (entry.SourceMakeCode == 0 ||
            entry.SourceMakeCode > MKFILTER_SCAN_CODE_MAX ||
            entry.TargetMakeCode == 0 ||
            entry.TargetMakeCode > MKFILTER_SCAN_CODE_MAX ||
            !MkFilterMappingPrefixIsValid(entry.SourcePrefix) ||
            !MkFilterMappingPrefixIsValid(entry.TargetPrefix))
        {
            return MkFilterMappingInvalidEntry;
        }

        sourceKey = MkFilterMappingSourceKey(
            entry.SourceMakeCode,
            entry.SourcePrefix
            );

        if (index != 0 && sourceKey <= previousKey)
        {
            return MkFilterMappingEntriesNotSorted;
        }

        candidate.Entries[index] = entry;
        previousKey = sourceKey;
    }

    *output = candidate;
    return MkFilterMappingSuccess;
}

int
MkFilterMappingTranslate(
    const MKFILTER_MAPPING_TABLE *table,
    uint16_t sourceMakeCode,
    uint16_t sourcePrefix,
    uint16_t *targetMakeCode,
    uint16_t *targetPrefix
    )
{
    uint32_t low;
    uint32_t high;
    uint32_t sourceKey;
    size_t expectedSize;

    if (table == NULL || targetMakeCode == NULL || targetPrefix == NULL)
    {
        return 0;
    }

    if (table->Header.Version != (uint32_t)MKFILTER_MAPPING_VERSION ||
        table->Header.EntryCount >
            (uint32_t)MKFILTER_MAPPING_MAX_ENTRIES ||
        table->Header.Flags !=
            (uint32_t)MKFILTER_MAPPING_FLAGS_NONE)
    {
        return 0;
    }

    expectedSize = MkFilterMappingSerializedSize(table->Header.EntryCount);
    if (expectedSize == 0 || table->Header.Size != expectedSize)
    {
        return 0;
    }

    sourceKey = MkFilterMappingSourceKey(sourceMakeCode, sourcePrefix);
    low = 0;
    high = table->Header.EntryCount;

    while (low < high)
    {
        const MKFILTER_MAPPING_ENTRY *entry;
        uint32_t middle;
        uint32_t entryKey;

        middle = low + ((high - low) / 2);
        entry = &table->Entries[middle];
        entryKey = MkFilterMappingSourceKey(
            entry->SourceMakeCode,
            entry->SourcePrefix
            );

        if (entryKey < sourceKey)
        {
            low = middle + 1;
        }
        else
        {
            high = middle;
        }
    }

    if (low >= table->Header.EntryCount)
    {
        return 0;
    }

    if (MkFilterMappingSourceKey(
            table->Entries[low].SourceMakeCode,
            table->Entries[low].SourcePrefix
            ) != sourceKey)
    {
        return 0;
    }

    if (table->Entries[low].TargetMakeCode == 0 ||
        table->Entries[low].TargetMakeCode > MKFILTER_SCAN_CODE_MAX ||
        !MkFilterMappingPrefixIsValid(table->Entries[low].TargetPrefix))
    {
        return 0;
    }

    *targetMakeCode = table->Entries[low].TargetMakeCode;
    *targetPrefix = table->Entries[low].TargetPrefix;
    return 1;
}

int
MkFilterMappingApplyPacket(
    const MKFILTER_MAPPING_TABLE *table,
    MKFILTER_KEY_PACKET *packet
    )
{
    uint16_t targetMakeCode;
    uint16_t targetPrefix;
    uint16_t sourcePrefix;

    if (packet == NULL)
    {
        return 0;
    }

    sourcePrefix = packet->Flags & MKFILTER_PREFIX_MASK;
    if (!MkFilterMappingTranslate(
            table,
            packet->MakeCode,
            sourcePrefix,
            &targetMakeCode,
            &targetPrefix
            ))
    {
        return 0;
    }

    packet->MakeCode = targetMakeCode;
    packet->Flags = (uint16_t)(
        (packet->Flags & (uint16_t)~MKFILTER_PREFIX_MASK) | targetPrefix
        );
    return 1;
}
