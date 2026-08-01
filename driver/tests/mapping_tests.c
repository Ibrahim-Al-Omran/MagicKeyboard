#include "mkfilter_mapping.h"
#include "mkfilter_hid_report.h"

#include <stdio.h>
#include <string.h>

static int failures;

#define CHECK(expression)                                                     \
    do                                                                        \
    {                                                                         \
        if (!(expression))                                                    \
        {                                                                     \
            (void)fprintf(                                                    \
                stderr,                                                       \
                "%s:%d: check failed: %s\n",                                  \
                __FILE__,                                                     \
                __LINE__,                                                     \
                #expression                                                   \
                );                                                            \
            ++failures;                                                       \
        }                                                                     \
    } while (0)

static MKFILTER_MAPPING_TABLE
MakeValidTable(
    uint32_t count
    )
{
    MKFILTER_MAPPING_TABLE table;
    uint32_t index;

    MkFilterMappingInitialize(&table);
    table.Header.EntryCount = count;
    table.Header.Size = (uint32_t)MkFilterMappingSerializedSize(count);

    for (index = 0; index < count; ++index)
    {
        table.Entries[index].SourceMakeCode = (uint16_t)(index + 1);
        table.Entries[index].SourcePrefix = MKFILTER_PREFIX_NONE;
        table.Entries[index].TargetMakeCode = (uint16_t)(
            MKFILTER_SCAN_CODE_MAX - index
            );
        table.Entries[index].TargetPrefix = MKFILTER_PREFIX_NONE;
    }

    return table;
}

static void
TestInitializationAndSizes(void)
{
    MKFILTER_MAPPING_TABLE table;

    (void)memset(&table, 0xa5, sizeof(table));
    MkFilterMappingInitialize(&table);

    CHECK(sizeof(MKFILTER_MAPPING_HEADER) == (size_t)16);
    CHECK(sizeof(MKFILTER_MAPPING_ENTRY) == (size_t)8);
    CHECK(table.Header.Size ==
          (uint32_t)sizeof(MKFILTER_MAPPING_HEADER));
    CHECK(table.Header.Version == (uint32_t)MKFILTER_MAPPING_VERSION);
    CHECK(table.Header.EntryCount == 0);
    CHECK(table.Header.Flags == (uint32_t)MKFILTER_MAPPING_FLAGS_NONE);
    CHECK(table.Entries[0].SourceMakeCode == 0);
    CHECK(MkFilterMappingSerializedSize(0) ==
          sizeof(MKFILTER_MAPPING_HEADER));
    CHECK(MkFilterMappingSerializedSize(MKFILTER_MAPPING_MAX_ENTRIES) ==
          sizeof(MKFILTER_MAPPING_TABLE));
    CHECK(MkFilterMappingSerializedSize(
              MKFILTER_MAPPING_MAX_ENTRIES + 1
              ) == 0);
}

static void
TestValidationAcceptsBoundedSortedTable(void)
{
    MKFILTER_MAPPING_TABLE input;
    MKFILTER_MAPPING_TABLE output;
    MKFILTER_MAPPING_STATUS status;

    input = MakeValidTable(3);
    (void)memset(&output, 0xcc, sizeof(output));

    status = MkFilterMappingValidateAndCopy(
        &input,
        input.Header.Size,
        &output
        );

    CHECK(status == MkFilterMappingSuccess);
    CHECK(output.Header.EntryCount == 3);
    CHECK(output.Entries[0].SourceMakeCode == 1);
    CHECK(output.Entries[2].SourceMakeCode == 3);
    CHECK(output.Entries[3].SourceMakeCode == 0);

    input = MakeValidTable(MKFILTER_MAPPING_MAX_ENTRIES);
    status = MkFilterMappingValidateAndCopy(
        &input,
        input.Header.Size,
        &output
        );
    CHECK(status == MkFilterMappingSuccess);
    CHECK(output.Header.EntryCount ==
          (uint32_t)MKFILTER_MAPPING_MAX_ENTRIES);
}

static void
TestValidationRejectsMalformedHeaders(void)
{
    MKFILTER_MAPPING_TABLE input;
    MKFILTER_MAPPING_TABLE output;
    MKFILTER_MAPPING_TABLE before;

    input = MakeValidTable(1);
    (void)memset(&output, 0x5a, sizeof(output));
    before = output;

    CHECK(MkFilterMappingValidateAndCopy(
              NULL,
              input.Header.Size,
              &output
              ) == MkFilterMappingInvalidArgument);
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              NULL
              ) == MkFilterMappingInvalidArgument);
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              sizeof(MKFILTER_MAPPING_HEADER) - 1,
              &output
              ) == MkFilterMappingLengthMismatch);

    input.Header.Version = MKFILTER_MAPPING_VERSION + 1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingUnsupportedVersion);

    input = MakeValidTable(1);
    input.Header.Flags = 1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingInvalidFlags);

    input = MakeValidTable(1);
    ++input.Header.Size;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingLengthMismatch);

    input = MakeValidTable(1);
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size + 1,
              &output
              ) == MkFilterMappingLengthMismatch);

    input = MakeValidTable(0);
    input.Header.EntryCount = MKFILTER_MAPPING_MAX_ENTRIES + 1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              sizeof(MKFILTER_MAPPING_HEADER),
              &output
              ) == MkFilterMappingTooManyEntries);

    CHECK(memcmp(&output, &before, sizeof(output)) == 0);
}

static void
TestValidationRejectsInvalidEntries(void)
{
    MKFILTER_MAPPING_TABLE input;
    MKFILTER_MAPPING_TABLE output;

    input = MakeValidTable(1);
    input.Entries[0].SourceMakeCode = 0;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingInvalidEntry);

    input = MakeValidTable(1);
    input.Entries[0].TargetMakeCode = MKFILTER_SCAN_CODE_MAX + 1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingInvalidEntry);

    input = MakeValidTable(1);
    input.Entries[0].SourcePrefix =
        MKFILTER_PREFIX_E0 | MKFILTER_PREFIX_E1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingInvalidEntry);

    input = MakeValidTable(1);
    input.Entries[0].TargetPrefix = 1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingInvalidEntry);
}

static void
TestValidationRequiresUniqueSortedSources(void)
{
    MKFILTER_MAPPING_TABLE input;
    MKFILTER_MAPPING_TABLE output;

    input = MakeValidTable(2);
    input.Entries[1].SourceMakeCode = input.Entries[0].SourceMakeCode;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingEntriesNotSorted);

    input = MakeValidTable(2);
    input.Entries[0].SourceMakeCode = 2;
    input.Entries[1].SourceMakeCode = 1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingEntriesNotSorted);

    input = MakeValidTable(2);
    input.Entries[0].SourceMakeCode = 1;
    input.Entries[0].SourcePrefix = MKFILTER_PREFIX_E0;
    input.Entries[1].SourceMakeCode = 1;
    input.Entries[1].SourcePrefix = MKFILTER_PREFIX_E1;
    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &output
              ) == MkFilterMappingSuccess);
}

static void
TestPacketMappingPreservesNonPrefixFlags(void)
{
    MKFILTER_MAPPING_TABLE input;
    MKFILTER_MAPPING_TABLE table;
    MKFILTER_KEY_PACKET packet;
    uint16_t targetCode;
    uint16_t targetPrefix;

    input = MakeValidTable(2);
    input.Entries[0].SourceMakeCode = 0x1e;
    input.Entries[0].SourcePrefix = MKFILTER_PREFIX_NONE;
    input.Entries[0].TargetMakeCode = 0x30;
    input.Entries[0].TargetPrefix = MKFILTER_PREFIX_E0;
    input.Entries[1].SourceMakeCode = 0x20;
    input.Entries[1].SourcePrefix = MKFILTER_PREFIX_E0;
    input.Entries[1].TargetMakeCode = 0x21;
    input.Entries[1].TargetPrefix = MKFILTER_PREFIX_NONE;
    targetCode = 0;
    targetPrefix = 0;

    CHECK(MkFilterMappingValidateAndCopy(
              &input,
              input.Header.Size,
              &table
              ) == MkFilterMappingSuccess);

    CHECK(MkFilterMappingTranslate(
              &table,
              0x20,
              MKFILTER_PREFIX_E0,
              &targetCode,
              &targetPrefix
              ) == 1);
    CHECK(targetCode == 0x21);
    CHECK(targetPrefix == MKFILTER_PREFIX_NONE);

    packet.MakeCode = 0x1e;
    packet.Flags = 0x0001;
    CHECK(MkFilterMappingApplyPacket(&table, &packet) == 1);
    CHECK(packet.MakeCode == 0x30);
    CHECK(packet.Flags == (uint16_t)(0x0001 | MKFILTER_PREFIX_E0));

    packet.MakeCode = 0x20;
    packet.Flags = (uint16_t)(0x0011 | MKFILTER_PREFIX_E0);
    CHECK(MkFilterMappingApplyPacket(&table, &packet) == 1);
    CHECK(packet.MakeCode == 0x21);
    CHECK(packet.Flags == 0x0011);

    packet.MakeCode = 0x22;
    packet.Flags = 0x0001;
    CHECK(MkFilterMappingApplyPacket(&table, &packet) == 0);
    CHECK(packet.MakeCode == 0x22);
    CHECK(packet.Flags == 0x0001);
}

static void
TestCorruptRuntimeTableFailsClosedToPassThrough(void)
{
    MKFILTER_MAPPING_TABLE table;
    MKFILTER_KEY_PACKET packet;

    table = MakeValidTable(1);
    table.Header.EntryCount = MKFILTER_MAPPING_MAX_ENTRIES + 1;
    packet.MakeCode = 1;
    packet.Flags = 0;

    CHECK(MkFilterMappingApplyPacket(&table, &packet) == 0);
    CHECK(packet.MakeCode == 1);
    CHECK(packet.Flags == 0);

    table = MakeValidTable(1);
    table.Entries[0].TargetMakeCode = 0;
    CHECK(MkFilterMappingApplyPacket(&table, &packet) == 0);
    CHECK(packet.MakeCode == 1);
    CHECK(packet.Flags == 0);
}

static void
TestHidEjectRewritesToDelete(void)
{
    uint8_t report[10] = {
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01
    };

    CHECK(MkFilterHidRewriteEjectToDelete(report, sizeof(report)) == 1);
    CHECK(report[9] == 0x00);
    CHECK(report[3] == MKFILTER_HID_DELETE_USAGE);
    CHECK(MkFilterHidRewriteEjectToDelete(report, sizeof(report)) == 0);
}

static void
TestHidEjectIgnoredWithoutBit(void)
{
    uint8_t report[10] = {
        0x01, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
    };

    CHECK(MkFilterHidRewriteEjectToDelete(report, sizeof(report)) == 0);
    CHECK(report[3] == 0x04);
}

static void
TestA1644PayloadEjectAndFnSwap(void)
{
    MKFILTER_A1644_STATE state;
    uint8_t payload[9] = {
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01
    };

    MkFilterA1644StateInit(&state, 0, 0);
    MkFilterProcessA1644Payload(payload, sizeof(payload), &state);
    CHECK(payload[2] == (uint8_t)MkFilterHidDel);
    CHECK(payload[8] == 0x00);

    MkFilterA1644StateInit(&state, 1, 0);
    payload[0] = (uint8_t)MkFilterHidLCtrlMask;
    payload[2] = 0;
    payload[8] = 0x02; /* physical Fn held with LCtrl → becomes RCtrl */
    MkFilterProcessA1644Payload(payload, sizeof(payload), &state);
    CHECK((payload[0] & (uint8_t)MkFilterHidRCtrlMask) != 0);
    CHECK((payload[0] & (uint8_t)MkFilterHidLCtrlMask) == 0);
    CHECK(payload[8] == 0x00);
}

int
main(void)
{
    TestInitializationAndSizes();
    TestValidationAcceptsBoundedSortedTable();
    TestValidationRejectsMalformedHeaders();
    TestValidationRejectsInvalidEntries();
    TestValidationRequiresUniqueSortedSources();
    TestPacketMappingPreservesNonPrefixFlags();
    TestCorruptRuntimeTableFailsClosedToPassThrough();
    TestHidEjectRewritesToDelete();
    TestHidEjectIgnoredWithoutBit();
    TestA1644PayloadEjectAndFnSwap();

    if (failures != 0)
    {
        (void)fprintf(stderr, "%d mapping test(s) failed\n", failures);
        return 1;
    }

    (void)printf("all mapping tests passed\n");
    return 0;
}
