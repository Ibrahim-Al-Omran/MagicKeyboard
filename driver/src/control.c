#include "mkfilter_driver.h"

static NTSTATUS
MkFilterMappingStatusToNtStatus(
    MKFILTER_MAPPING_STATUS mappingStatus
    )
{
    switch (mappingStatus)
    {
    case MkFilterMappingSuccess:
        return STATUS_SUCCESS;

    case MkFilterMappingLengthMismatch:
        return STATUS_INFO_LENGTH_MISMATCH;

    case MkFilterMappingUnsupportedVersion:
        return STATUS_REVISION_MISMATCH;

    case MkFilterMappingTooManyEntries:
    case MkFilterMappingInvalidFlags:
    case MkFilterMappingInvalidEntry:
    case MkFilterMappingEntriesNotSorted:
    case MkFilterMappingInvalidArgument:
    default:
        return STATUS_INVALID_PARAMETER;
    }
}

static VOID
MkFilterAdvanceGeneration(
    MKFILTER_DRIVER_CONTEXT *context
    )
{
    ++context->MappingGeneration;
    if (context->MappingGeneration == 0)
    {
        ++context->MappingGeneration;
    }
}

static VOID
MkFilterCompleteCapabilities(
    WDFREQUEST request,
    size_t inputBufferLength,
    size_t outputBufferLength,
    MKFILTER_DRIVER_CONTEXT *context
    )
{
    MKFILTER_CAPABILITIES *capabilities;
    size_t bufferLength;
    NTSTATUS status;

    if (inputBufferLength != 0 ||
        outputBufferLength < sizeof(*capabilities))
    {
        WdfRequestCompleteWithInformation(
            request,
            STATUS_INFO_LENGTH_MISMATCH,
            sizeof(*capabilities)
            );
        return;
    }

    status = WdfRequestRetrieveOutputBuffer(
        request,
        sizeof(*capabilities),
        (PVOID *)&capabilities,
        &bufferLength
        );
    if (!NT_SUCCESS(status))
    {
        WdfRequestComplete(request, status);
        return;
    }

    RtlZeroMemory(capabilities, sizeof(*capabilities));
    capabilities->Size = (uint32_t)sizeof(*capabilities);
    capabilities->ProtocolVersion = MKFILTER_PROTOCOL_VERSION;
    capabilities->MappingVersion = MKFILTER_MAPPING_VERSION;
    capabilities->MaxEntries = MKFILTER_MAPPING_MAX_ENTRIES;
    capabilities->DriverVersionMajor = 0;
    capabilities->DriverVersionMinor = 1;
    capabilities->DriverVersionPatch = 0;
    capabilities->Flags = MKFILTER_CAPABILITY_BOUNDED_TABLE;

    WdfSpinLockAcquire(context->MappingLock);
    capabilities->MappingGeneration = context->MappingGeneration;
    WdfSpinLockRelease(context->MappingLock);

    WdfRequestCompleteWithInformation(
        request,
        STATUS_SUCCESS,
        sizeof(*capabilities)
        );
}

static VOID
MkFilterCompleteGetMapping(
    WDFREQUEST request,
    size_t inputBufferLength,
    size_t outputBufferLength,
    MKFILTER_DRIVER_CONTEXT *context
    )
{
    MKFILTER_MAPPING_TABLE snapshot;
    void *output;
    size_t bufferLength;
    size_t requiredLength;
    NTSTATUS status;

    if (inputBufferLength != 0)
    {
        WdfRequestComplete(request, STATUS_INFO_LENGTH_MISMATCH);
        return;
    }

    WdfSpinLockAcquire(context->MappingLock);
    snapshot = context->Mapping;
    WdfSpinLockRelease(context->MappingLock);

    requiredLength = MkFilterMappingSerializedSize(
        snapshot.Header.EntryCount
        );
    if (requiredLength == 0)
    {
        WdfRequestComplete(request, STATUS_DATA_ERROR);
        return;
    }

    if (outputBufferLength < requiredLength)
    {
        WdfRequestCompleteWithInformation(
            request,
            STATUS_BUFFER_TOO_SMALL,
            requiredLength
            );
        return;
    }

    status = WdfRequestRetrieveOutputBuffer(
        request,
        requiredLength,
        &output,
        &bufferLength
        );
    if (!NT_SUCCESS(status))
    {
        WdfRequestComplete(request, status);
        return;
    }

    RtlCopyMemory(output, &snapshot, requiredLength);
    WdfRequestCompleteWithInformation(
        request,
        STATUS_SUCCESS,
        requiredLength
        );
}

static VOID
MkFilterCompleteSetMapping(
    WDFREQUEST request,
    size_t inputBufferLength,
    size_t outputBufferLength,
    MKFILTER_DRIVER_CONTEXT *context
    )
{
    MKFILTER_MAPPING_TABLE candidate;
    MKFILTER_MAPPING_STATUS mappingStatus;
    void *input;
    size_t bufferLength;
    NTSTATUS status;

    if (outputBufferLength != 0 ||
        inputBufferLength < sizeof(MKFILTER_MAPPING_HEADER))
    {
        WdfRequestComplete(request, STATUS_INFO_LENGTH_MISMATCH);
        return;
    }

    status = WdfRequestRetrieveInputBuffer(
        request,
        sizeof(MKFILTER_MAPPING_HEADER),
        &input,
        &bufferLength
        );
    if (!NT_SUCCESS(status))
    {
        WdfRequestComplete(request, status);
        return;
    }

    if (bufferLength != inputBufferLength)
    {
        WdfRequestComplete(request, STATUS_INFO_LENGTH_MISMATCH);
        return;
    }

    mappingStatus = MkFilterMappingValidateAndCopy(
        input,
        bufferLength,
        &candidate
        );
    status = MkFilterMappingStatusToNtStatus(mappingStatus);
    if (!NT_SUCCESS(status))
    {
        WdfRequestComplete(request, status);
        return;
    }

    WdfSpinLockAcquire(context->MappingLock);
    context->Mapping = candidate;
    MkFilterAdvanceGeneration(context);
    WdfSpinLockRelease(context->MappingLock);

    WdfRequestComplete(request, STATUS_SUCCESS);
}

static VOID
MkFilterCompleteClearMapping(
    WDFREQUEST request,
    size_t inputBufferLength,
    size_t outputBufferLength,
    MKFILTER_DRIVER_CONTEXT *context
    )
{
    MKFILTER_MAPPING_TABLE empty;

    if (inputBufferLength != 0 || outputBufferLength != 0)
    {
        WdfRequestComplete(request, STATUS_INFO_LENGTH_MISMATCH);
        return;
    }

    MkFilterMappingInitialize(&empty);

    WdfSpinLockAcquire(context->MappingLock);
    context->Mapping = empty;
    MkFilterAdvanceGeneration(context);
    WdfSpinLockRelease(context->MappingLock);

    WdfRequestComplete(request, STATUS_SUCCESS);
}

VOID
MkFilterEvtIoDeviceControl(
    WDFQUEUE queue,
    WDFREQUEST request,
    size_t outputBufferLength,
    size_t inputBufferLength,
    ULONG ioControlCode
    )
{
    WDFDEVICE controlDevice;
    WDFDRIVER driver;
    MKFILTER_DRIVER_CONTEXT *context;

    controlDevice = WdfIoQueueGetDevice(queue);
    driver = WdfDeviceGetDriver(controlDevice);
    context = MkFilterGetDriverContext(driver);

    if (!context->MappingReady || context->MappingLock == NULL)
    {
        WdfRequestComplete(request, STATUS_DEVICE_NOT_READY);
        return;
    }

    switch (ioControlCode)
    {
    case IOCTL_MKFILTER_QUERY_CAPABILITIES:
        MkFilterCompleteCapabilities(
            request,
            inputBufferLength,
            outputBufferLength,
            context
            );
        break;

    case IOCTL_MKFILTER_GET_MAPPING:
        MkFilterCompleteGetMapping(
            request,
            inputBufferLength,
            outputBufferLength,
            context
            );
        break;

    case IOCTL_MKFILTER_SET_MAPPING:
        MkFilterCompleteSetMapping(
            request,
            inputBufferLength,
            outputBufferLength,
            context
            );
        break;

    case IOCTL_MKFILTER_CLEAR_MAPPING:
        MkFilterCompleteClearMapping(
            request,
            inputBufferLength,
            outputBufferLength,
            context
            );
        break;

    default:
        WdfRequestComplete(request, STATUS_INVALID_DEVICE_REQUEST);
        break;
    }
}

NTSTATUS
MkFilterCreateControlDevice(
    WDFDRIVER driver
    )
{
    DECLARE_CONST_UNICODE_STRING(
        deviceName,
        L"\\Device\\MagicKeyboardFilterControl"
        );
    DECLARE_CONST_UNICODE_STRING(
        symbolicLink,
        L"\\DosDevices\\MagicKeyboardFilter"
        );
    DECLARE_CONST_UNICODE_STRING(
        securityDescriptor,
        L"D:P(A;;GA;;;SY)(A;;GA;;;BA)"
        );
    PWDFDEVICE_INIT controlInit;
    WDF_IO_QUEUE_CONFIG queueConfig;
    WDFDEVICE controlDevice;
    MKFILTER_DRIVER_CONTEXT *context;
    NTSTATUS status;

    context = MkFilterGetDriverContext(driver);
    controlInit = WdfControlDeviceInitAllocate(
        driver,
        &securityDescriptor
        );
    if (controlInit == NULL)
    {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    WdfDeviceInitSetDeviceType(
        controlInit,
        MKFILTER_IOCTL_DEVICE_TYPE
        );
    WdfDeviceInitSetCharacteristics(
        controlInit,
        FILE_DEVICE_SECURE_OPEN,
        TRUE
        );
    WdfDeviceInitSetExclusive(controlInit, FALSE);
    WdfDeviceInitSetIoType(controlInit, WdfDeviceIoBuffered);

    status = WdfDeviceInitAssignName(controlInit, &deviceName);
    if (!NT_SUCCESS(status))
    {
        WdfDeviceInitFree(controlInit);
        return status;
    }

    status = WdfDeviceCreate(
        &controlInit,
        WDF_NO_OBJECT_ATTRIBUTES,
        &controlDevice
        );
    if (!NT_SUCCESS(status))
    {
        if (controlInit != NULL)
        {
            WdfDeviceInitFree(controlInit);
        }
        return status;
    }

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(
        &queueConfig,
        WdfIoQueueDispatchSequential
        );
    queueConfig.PowerManaged = WdfFalse;
    queueConfig.EvtIoDeviceControl = MkFilterEvtIoDeviceControl;

    status = WdfIoQueueCreate(
        controlDevice,
        &queueConfig,
        WDF_NO_OBJECT_ATTRIBUTES,
        WDF_NO_HANDLE
        );
    if (!NT_SUCCESS(status))
    {
        WdfObjectDelete(controlDevice);
        return status;
    }

    status = WdfDeviceCreateSymbolicLink(
        controlDevice,
        &symbolicLink
        );
    if (!NT_SUCCESS(status))
    {
        WdfObjectDelete(controlDevice);
        return status;
    }

    context->ControlDevice = controlDevice;
    WdfControlFinishInitializing(controlDevice);
    return STATUS_SUCCESS;
}
