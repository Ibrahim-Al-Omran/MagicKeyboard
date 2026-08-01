#include "mkfilter_driver.h"

NTSTATUS
MkFilterEvtDeviceAdd(
    WDFDRIVER driver,
    PWDFDEVICE_INIT deviceInit
    )
{
    WDF_PNPPOWER_EVENT_CALLBACKS pnpPowerCallbacks;
    WDF_OBJECT_ATTRIBUTES deviceAttributes;
    MKFILTER_DEVICE_CONTEXT *context;
    WDFDEVICE device;
    NTSTATUS status;

    UNREFERENCED_PARAMETER(driver);

    /*
     * The INF is the attachment allowlist. WdfFdoInitSetFilter also causes
     * request types without a queue callback to pass through automatically.
     */
    WdfFdoInitSetFilter(deviceInit);
    WdfDeviceInitSetDeviceType(deviceInit, FILE_DEVICE_KEYBOARD);

    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&pnpPowerCallbacks);
    pnpPowerCallbacks.EvtDevicePrepareHardware =
        MkFilterEvtDevicePrepareHardware;
    pnpPowerCallbacks.EvtDeviceReleaseHardware =
        MkFilterEvtDeviceReleaseHardware;
    pnpPowerCallbacks.EvtDeviceD0Entry = MkFilterEvtDeviceD0Entry;
    pnpPowerCallbacks.EvtDeviceD0Exit = MkFilterEvtDeviceD0Exit;
    WdfDeviceInitSetPnpPowerEventCallbacks(
        deviceInit,
        &pnpPowerCallbacks
        );

    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(
        &deviceAttributes,
        MKFILTER_DEVICE_CONTEXT
        );

    status = WdfDeviceCreate(
        &deviceInit,
        &deviceAttributes,
        &device
        );
    if (!NT_SUCCESS(status))
    {
        return status;
    }

    context = MkFilterGetDeviceContext(device);
    context->ConnectRequestObserved = 0;
    RtlZeroMemory(&context->UpperConnectData, sizeof(context->UpperConnectData));

    /*
     * Queue creation is optional. Without it KMDF's filter behavior forwards
     * all requests. Returning success here keeps configuration failures from
     * blocking the underlying keyboard stack.
     */
    (void)MkFilterCreateInputQueue(device);
    return STATUS_SUCCESS;
}

NTSTATUS
MkFilterEvtDevicePrepareHardware(
    WDFDEVICE device,
    WDFCMRESLIST resourcesRaw,
    WDFCMRESLIST resourcesTranslated
    )
{
    UNREFERENCED_PARAMETER(device);
    UNREFERENCED_PARAMETER(resourcesRaw);
    UNREFERENCED_PARAMETER(resourcesTranslated);
    return STATUS_SUCCESS;
}

NTSTATUS
MkFilterEvtDeviceReleaseHardware(
    WDFDEVICE device,
    WDFCMRESLIST resourcesTranslated
    )
{
    UNREFERENCED_PARAMETER(device);
    UNREFERENCED_PARAMETER(resourcesTranslated);
    return STATUS_SUCCESS;
}

NTSTATUS
MkFilterEvtDeviceD0Entry(
    WDFDEVICE device,
    WDF_POWER_DEVICE_STATE previousState
    )
{
    UNREFERENCED_PARAMETER(device);
    UNREFERENCED_PARAMETER(previousState);
    return STATUS_SUCCESS;
}

NTSTATUS
MkFilterEvtDeviceD0Exit(
    WDFDEVICE device,
    WDF_POWER_DEVICE_STATE targetState
    )
{
    UNREFERENCED_PARAMETER(device);
    UNREFERENCED_PARAMETER(targetState);
    return STATUS_SUCCESS;
}
