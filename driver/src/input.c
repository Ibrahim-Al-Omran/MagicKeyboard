#include "mkfilter_driver.h"

static VOID
MkFilterForwardRequest(
    WDFDEVICE device,
    WDFREQUEST request
    )
{
    WDF_REQUEST_SEND_OPTIONS options;

    WdfRequestFormatRequestUsingCurrentType(request);

    WDF_REQUEST_SEND_OPTIONS_INIT(
        &options,
        WDF_REQUEST_SEND_OPTION_SEND_AND_FORGET
        );

    if (!WdfRequestSend(
            request,
            WdfDeviceGetIoTarget(device),
            &options
            ))
    {
        WdfRequestComplete(request, WdfRequestGetStatus(request));
    }
}

NTSTATUS
MkFilterCreateInputQueue(
    WDFDEVICE device
    )
{
    WDF_IO_QUEUE_CONFIG queueConfig;

    WDF_IO_QUEUE_CONFIG_INIT_DEFAULT_QUEUE(
        &queueConfig,
        WdfIoQueueDispatchParallel
        );
    queueConfig.EvtIoInternalDeviceControl =
        MkFilterEvtIoInternalDeviceControl;

    return WdfIoQueueCreate(
        device,
        &queueConfig,
        WDF_NO_OBJECT_ATTRIBUTES,
        WDF_NO_HANDLE
        );
}

VOID
MkFilterEvtIoInternalDeviceControl(
    WDFQUEUE queue,
    WDFREQUEST request,
    size_t outputBufferLength,
    size_t inputBufferLength,
    ULONG ioControlCode
    )
{
    WDFDEVICE device;
    MKFILTER_DEVICE_CONTEXT *context;

    UNREFERENCED_PARAMETER(outputBufferLength);
    UNREFERENCED_PARAMETER(inputBufferLength);

    device = WdfIoQueueGetDevice(queue);
    context = MkFilterGetDeviceContext(device);

    if (ioControlCode == IOCTL_INTERNAL_KEYBOARD_CONNECT)
    {
        /*
         * Observe only. Do not replace CONNECT_DATA.ClassService until the
         * callback lifecycle has been validated on each allowlisted device.
         */
        (void)InterlockedExchange(&context->ConnectRequestObserved, 1);
    }

    MkFilterForwardRequest(device, request);
}

VOID
MkFilterServiceCallback(
    PDEVICE_OBJECT deviceObject,
    PKEYBOARD_INPUT_DATA inputDataStart,
    PKEYBOARD_INPUT_DATA inputDataEnd,
    PULONG inputDataConsumed
    )
{
    WDFDEVICE device;
    WDFDRIVER driver;
    MKFILTER_DEVICE_CONTEXT *deviceContext;
    MKFILTER_DRIVER_CONTEXT *driverContext;
    MKFILTER_MAPPING_TABLE snapshot;
    PKEYBOARD_INPUT_DATA current;
    CONNECT_DATA upperConnectData;

    device = WdfWdmDeviceGetWdfDeviceHandle(deviceObject);
    driver = WdfDeviceGetDriver(device);
    deviceContext = MkFilterGetDeviceContext(device);
    driverContext = MkFilterGetDriverContext(driver);
    upperConnectData = deviceContext->UpperConnectData;

    if (driverContext->MappingReady &&
        driverContext->MappingLock != NULL &&
        inputDataStart != NULL &&
        inputDataEnd != NULL &&
        inputDataEnd >= inputDataStart)
    {
        WdfSpinLockAcquire(driverContext->MappingLock);
        snapshot = driverContext->Mapping;
        WdfSpinLockRelease(driverContext->MappingLock);

        for (current = inputDataStart;
             current < inputDataEnd;
             ++current)
        {
            MKFILTER_KEY_PACKET packet;

            packet.MakeCode = current->MakeCode;
            packet.Flags = current->Flags;
            if (MkFilterMappingApplyPacket(&snapshot, &packet))
            {
                current->MakeCode = packet.MakeCode;
                current->Flags = packet.Flags;
            }
        }
    }

    if (upperConnectData.ClassService != NULL)
    {
        ((PSERVICE_CALLBACK_ROUTINE)upperConnectData.ClassService)(
            upperConnectData.ClassDeviceObject,
            inputDataStart,
            inputDataEnd,
            inputDataConsumed
            );
    }
    else if (inputDataConsumed != NULL)
    {
        *inputDataConsumed = 0;
    }
}
