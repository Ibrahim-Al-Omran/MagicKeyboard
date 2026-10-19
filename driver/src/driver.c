/*
 * Magic Keyboard A1644 HIDCLASS lower filter (WDM).
 *
 * Architecture and IRP interception adapted from WinAppleKey (MIT License),
 * Copyright (c) 2021 George Samartzidis.
 * https://github.com/samartzidis/WinAppleKey
 */

#include "mkfilter_driver.h"

#ifdef ALLOC_PRAGMA
#pragma alloc_text(INIT, DriverEntry)
#pragma alloc_text(PAGE, MkFilterDriverUnload)
#pragma alloc_text(PAGE, MkFilterAddDevice)
#pragma alloc_text(PAGE, MkFilterRemoveDevice)
#pragma alloc_text(PAGE, MkFilterGetLowerDeviceType)
#pragma alloc_text(PAGE, MkFilterReadRegistryDword)
#endif

static ULONG g_SwapAltCmd = 0;
static ULONG g_SwapFnCtrl = 0;

NTSTATUS
DriverEntry(
    PDRIVER_OBJECT driverObject,
    PUNICODE_STRING registryPath
    )
{
    ULONG index;

    MkFilterDebugPrint("DriverEntry driverObject=%p\n", driverObject);

    driverObject->DriverUnload = MkFilterDriverUnload;
    driverObject->DriverExtension->AddDevice = MkFilterAddDevice;

    for (index = 0; index <= IRP_MJ_MAXIMUM_FUNCTION; ++index)
    {
        driverObject->MajorFunction[index] = MkFilterDispatchAny;
    }

    driverObject->MajorFunction[IRP_MJ_POWER] = MkFilterDispatchPower;
    driverObject->MajorFunction[IRP_MJ_PNP] = MkFilterDispatchPnp;
    driverObject->MajorFunction[IRP_MJ_INTERNAL_DEVICE_CONTROL] =
        MkFilterDispatchInternalIoctl;

    (void)MkFilterReadRegistryDword(registryPath, L"SwapAltCmd", &g_SwapAltCmd);
    (void)MkFilterReadRegistryDword(registryPath, L"SwapFnCtrl", &g_SwapFnCtrl);

    return STATUS_SUCCESS;
}

VOID
MkFilterDriverUnload(
    PDRIVER_OBJECT driverObject
    )
{
    PAGED_CODE();
    UNREFERENCED_PARAMETER(driverObject);
    MkFilterDebugPrint("DriverUnload\n");
}

NTSTATUS
MkFilterAddDevice(
    PDRIVER_OBJECT driverObject,
    PDEVICE_OBJECT pdo
    )
{
    PDEVICE_OBJECT fido = NULL;
    PMKFILTER_DEVICE_EXTENSION extension;
    PDEVICE_OBJECT lower;
    ULONG type;
    NTSTATUS status;

    PAGED_CODE();

    type = MkFilterGetLowerDeviceType(pdo);
    status = IoCreateDevice(
        driverObject,
        sizeof(MKFILTER_DEVICE_EXTENSION),
        NULL,
        type,
        0,
        FALSE,
        &fido
        );
    if (!NT_SUCCESS(status))
    {
        return status;
    }

    extension = (PMKFILTER_DEVICE_EXTENSION)fido->DeviceExtension;
    RtlZeroMemory(extension, sizeof(*extension));
    IoInitializeRemoveLock(&extension->RemoveLock, 0, 0, 0);
    extension->DeviceObject = fido;
    extension->Pdo = pdo;
    MkFilterA1644StateInit(
        &extension->A1644,
        g_SwapFnCtrl ? 1 : 0,
        g_SwapAltCmd ? 1 : 0
        );

    lower = IoAttachDeviceToDeviceStack(fido, pdo);
    if (lower == NULL)
    {
        IoDeleteDevice(fido);
        return STATUS_DEVICE_REMOVED;
    }

    extension->LowerDeviceObject = lower;
    fido->Flags |= lower->Flags & (DO_DIRECT_IO | DO_BUFFERED_IO | DO_POWER_PAGABLE);
    fido->Flags &= ~DO_DEVICE_INITIALIZING;
    return STATUS_SUCCESS;
}

void
MkFilterRemoveDevice(
    PDEVICE_OBJECT fido
    )
{
    PMKFILTER_DEVICE_EXTENSION extension;

    PAGED_CODE();

    extension = (PMKFILTER_DEVICE_EXTENSION)fido->DeviceExtension;
    if (extension->LowerDeviceObject != NULL)
    {
        IoDetachDevice(extension->LowerDeviceObject);
        extension->LowerDeviceObject = NULL;
    }
    IoDeleteDevice(fido);
}

ULONG
MkFilterGetLowerDeviceType(
    PDEVICE_OBJECT pdo
    )
{
    PDEVICE_OBJECT lowerPdo;
    ULONG type;

    PAGED_CODE();

    lowerPdo = IoGetAttachedDeviceReference(pdo);
    if (lowerPdo == NULL)
    {
        return FILE_DEVICE_UNKNOWN;
    }

    type = lowerPdo->DeviceType;
    ObDereferenceObject(lowerPdo);
    return type;
}

NTSTATUS
MkFilterCompleteRequest(
    PIRP irp,
    NTSTATUS status,
    ULONG_PTR info
    )
{
    irp->IoStatus.Status = status;
    irp->IoStatus.Information = info;
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    return status;
}

NTSTATUS
MkFilterDispatchPower(
    PDEVICE_OBJECT fido,
    PIRP irp
    )
{
    PMKFILTER_DEVICE_EXTENSION extension;
    NTSTATUS status;

    extension = (PMKFILTER_DEVICE_EXTENSION)fido->DeviceExtension;
    PoStartNextPowerIrp(irp);

    status = IoAcquireRemoveLock(&extension->RemoveLock, irp);
    if (!NT_SUCCESS(status))
    {
        return MkFilterCompleteRequest(irp, status, 0);
    }

    IoSkipCurrentIrpStackLocation(irp);
    status = PoCallDriver(extension->LowerDeviceObject, irp);
    IoReleaseRemoveLock(&extension->RemoveLock, irp);
    return status;
}

static NTSTATUS
MkFilterStartDeviceCompletion(
    PDEVICE_OBJECT fido,
    PIRP irp,
    PVOID context
    )
{
    PMKFILTER_DEVICE_EXTENSION extension;

    UNREFERENCED_PARAMETER(fido);
    extension = (PMKFILTER_DEVICE_EXTENSION)context;

    if (irp->PendingReturned)
    {
        IoMarkIrpPending(irp);
    }

    if ((extension->LowerDeviceObject->Characteristics & FILE_REMOVABLE_MEDIA) != 0)
    {
        fido->Characteristics |= FILE_REMOVABLE_MEDIA;
    }

    IoReleaseRemoveLock(&extension->RemoveLock, irp);
    return STATUS_SUCCESS;
}

static NTSTATUS
MkFilterUsageNotificationCompletion(
    PDEVICE_OBJECT fido,
    PIRP irp,
    PVOID context
    )
{
    PMKFILTER_DEVICE_EXTENSION extension;

    extension = (PMKFILTER_DEVICE_EXTENSION)context;

    if (irp->PendingReturned)
    {
        IoMarkIrpPending(irp);
    }

    if ((extension->LowerDeviceObject->Flags & DO_POWER_PAGABLE) == 0)
    {
        fido->Flags &= ~DO_POWER_PAGABLE;
    }

    IoReleaseRemoveLock(&extension->RemoveLock, irp);
    return STATUS_SUCCESS;
}

NTSTATUS
MkFilterDispatchPnp(
    PDEVICE_OBJECT fido,
    PIRP irp
    )
{
    PIO_STACK_LOCATION stack;
    PMKFILTER_DEVICE_EXTENSION extension;
    ULONG minor;
    NTSTATUS status;

    stack = IoGetCurrentIrpStackLocation(irp);
    minor = stack->MinorFunction;
    extension = (PMKFILTER_DEVICE_EXTENSION)fido->DeviceExtension;

    status = IoAcquireRemoveLock(&extension->RemoveLock, irp);
    if (!NT_SUCCESS(status))
    {
        return MkFilterCompleteRequest(irp, status, 0);
    }

    if (minor == IRP_MN_DEVICE_USAGE_NOTIFICATION)
    {
        if (fido->AttachedDevice == NULL ||
            (fido->AttachedDevice->Flags & DO_POWER_PAGABLE) != 0)
        {
            fido->Flags |= DO_POWER_PAGABLE;
        }

        IoCopyCurrentIrpStackLocationToNext(irp);
        IoSetCompletionRoutine(
            irp,
            MkFilterUsageNotificationCompletion,
            extension,
            TRUE,
            TRUE,
            TRUE
            );
        return IoCallDriver(extension->LowerDeviceObject, irp);
    }

    if (minor == IRP_MN_START_DEVICE)
    {
        IoCopyCurrentIrpStackLocationToNext(irp);
        IoSetCompletionRoutine(
            irp,
            MkFilterStartDeviceCompletion,
            extension,
            TRUE,
            TRUE,
            TRUE
            );
        return IoCallDriver(extension->LowerDeviceObject, irp);
    }

    if (minor == IRP_MN_REMOVE_DEVICE)
    {
        IoSkipCurrentIrpStackLocation(irp);
        status = IoCallDriver(extension->LowerDeviceObject, irp);
        IoReleaseRemoveLockAndWait(&extension->RemoveLock, irp);
        MkFilterRemoveDevice(fido);
        return status;
    }

    IoSkipCurrentIrpStackLocation(irp);
    status = IoCallDriver(extension->LowerDeviceObject, irp);
    IoReleaseRemoveLock(&extension->RemoveLock, irp);
    return status;
}

NTSTATUS
MkFilterDispatchAny(
    PDEVICE_OBJECT fido,
    PIRP irp
    )
{
    PMKFILTER_DEVICE_EXTENSION extension;
    NTSTATUS status;

    extension = (PMKFILTER_DEVICE_EXTENSION)fido->DeviceExtension;
    status = IoAcquireRemoveLock(&extension->RemoveLock, irp);
    if (!NT_SUCCESS(status))
    {
        return MkFilterCompleteRequest(irp, status, 0);
    }

    IoSkipCurrentIrpStackLocation(irp);
    status = IoCallDriver(extension->LowerDeviceObject, irp);
    IoReleaseRemoveLock(&extension->RemoveLock, irp);
    return status;
}

/*
 * Transfers may describe their data with either a flat pointer or an MDL.
 * Runs at <= DISPATCH_LEVEL, so only the non-blocking MDL mapping is allowed.
 */
static PUCHAR
MkFilterTransferBuffer(
    PVOID buffer,
    PMDL mdl,
    ULONG size
    )
{
    if (buffer != NULL)
    {
        return (PUCHAR)buffer;
    }

    if (mdl != NULL && MmGetMdlByteCount(mdl) >= size)
    {
        return (PUCHAR)MmGetSystemAddressForMdlSafe(
            mdl,
            NormalPagePriority | MdlMappingNoExecute
            );
    }

    return NULL;
}

static NTSTATUS
MkFilterInternalIoctlComplete(
    PDEVICE_OBJECT fido,
    PIRP irp,
    PVOID context
    )
{
    PIO_STACK_LOCATION stack;
    PMKFILTER_DEVICE_EXTENSION extension;
    ULONG control;

    UNREFERENCED_PARAMETER(context);
    extension = (PMKFILTER_DEVICE_EXTENSION)fido->DeviceExtension;
    stack = IoGetCurrentIrpStackLocation(irp);

    if (NT_SUCCESS(irp->IoStatus.Status))
    {
        control = stack->Parameters.DeviceIoControl.IoControlCode;
        if (control == IOCTL_INTERNAL_BTH_SUBMIT_BRB)
        {
            PBRB brb = (PBRB)stack->Parameters.Others.Argument1;
            if (brb != NULL &&
                brb->BrbHeader.Type == BRB_L2CA_ACL_TRANSFER &&
                (brb->BrbL2caAclTransfer.TransferFlags & ACL_TRANSFER_DIRECTION_IN) != 0)
            {
                ULONG size = brb->BrbL2caAclTransfer.BufferSize;
                PUCHAR buf = MkFilterTransferBuffer(
                    brb->BrbL2caAclTransfer.Buffer,
                    brb->BrbL2caAclTransfer.BufferMDL,
                    size
                    );
                /* HIDP input DATA header (0xA1), report ID 1, 9-byte keyboard payload. */
                if (buf != NULL && size == 11 && buf[0] == 0xA1 && buf[1] == 0x01)
                {
                    MkFilterProcessA1644Payload(buf + 2, 9, &extension->A1644);
                }
            }
        }
        else if (control == IOCTL_INTERNAL_USB_SUBMIT_URB)
        {
            PURB urb = (PURB)stack->Parameters.Others.Argument1;
            if (urb != NULL &&
                urb->UrbHeader.Function == URB_FUNCTION_BULK_OR_INTERRUPT_TRANSFER &&
                (urb->UrbBulkOrInterruptTransfer.TransferFlags & USBD_TRANSFER_DIRECTION_IN) != 0)
            {
                ULONG size = urb->UrbBulkOrInterruptTransfer.TransferBufferLength;
                PUCHAR buf = MkFilterTransferBuffer(
                    urb->UrbBulkOrInterruptTransfer.TransferBuffer,
                    urb->UrbBulkOrInterruptTransfer.TransferBufferMDL,
                    size
                    );
                if (buf != NULL && size == 10 && buf[0] == 0x01)
                {
                    MkFilterProcessA1644Payload(buf + 1, 9, &extension->A1644);
                }
            }
        }
    }

    if (irp->PendingReturned)
    {
        IoMarkIrpPending(irp);
    }

    IoReleaseRemoveLock(&extension->RemoveLock, irp);
    return STATUS_CONTINUE_COMPLETION;
}

NTSTATUS
MkFilterDispatchInternalIoctl(
    PDEVICE_OBJECT fido,
    PIRP irp
    )
{
    PMKFILTER_DEVICE_EXTENSION extension;
    PIO_STACK_LOCATION stack;
    ULONG control;
    BOOLEAN hookCompletion = FALSE;
    NTSTATUS status;

    extension = (PMKFILTER_DEVICE_EXTENSION)fido->DeviceExtension;
    stack = IoGetCurrentIrpStackLocation(irp);
    control = stack->Parameters.DeviceIoControl.IoControlCode;

    status = IoAcquireRemoveLock(&extension->RemoveLock, irp);
    if (!NT_SUCCESS(status))
    {
        return MkFilterCompleteRequest(irp, status, 0);
    }

    if (control == IOCTL_INTERNAL_BTH_SUBMIT_BRB)
    {
        PBRB brb = (PBRB)stack->Parameters.Others.Argument1;
        hookCompletion = brb != NULL && brb->BrbHeader.Type == BRB_L2CA_ACL_TRANSFER;
    }
    else if (control == IOCTL_INTERNAL_USB_SUBMIT_URB)
    {
        hookCompletion = TRUE;
    }

    if (!hookCompletion)
    {
        IoSkipCurrentIrpStackLocation(irp);
        status = IoCallDriver(extension->LowerDeviceObject, irp);
        IoReleaseRemoveLock(&extension->RemoveLock, irp);
        return status;
    }

    /* The completion routine releases the remove lock. */
    IoCopyCurrentIrpStackLocationToNext(irp);
    IoSetCompletionRoutine(
        irp,
        MkFilterInternalIoctlComplete,
        fido,
        TRUE,
        TRUE,
        TRUE
        );
    return IoCallDriver(extension->LowerDeviceObject, irp);
}

NTSTATUS
MkFilterReadRegistryDword(
    PUNICODE_STRING registryPath,
    PCWSTR valueName,
    PULONG value
    )
{
    HANDLE key = NULL;
    OBJECT_ATTRIBUTES attributes;
    UNICODE_STRING name;
    ULONG size = 0;
    NTSTATUS status;
    PKEY_VALUE_PARTIAL_INFORMATION info = NULL;

    PAGED_CODE();

    if (registryPath == NULL || valueName == NULL || value == NULL)
    {
        return STATUS_INVALID_PARAMETER;
    }

    InitializeObjectAttributes(
        &attributes,
        registryPath,
        OBJ_CASE_INSENSITIVE | OBJ_KERNEL_HANDLE,
        NULL,
        NULL
        );

    status = ZwOpenKey(&key, KEY_READ, &attributes);
    if (!NT_SUCCESS(status))
    {
        return status;
    }

    RtlInitUnicodeString(&name, valueName);
    status = ZwQueryValueKey(
        key,
        &name,
        KeyValuePartialInformation,
        NULL,
        0,
        &size
        );
    if (status != STATUS_BUFFER_TOO_SMALL && status != STATUS_BUFFER_OVERFLOW)
    {
        ZwClose(key);
        return status;
    }

#pragma warning(suppress: 4996)
    info = (PKEY_VALUE_PARTIAL_INFORMATION)ExAllocatePoolWithTag(
        PagedPool,
        size,
        'fKkM'
        );
    if (info == NULL)
    {
        ZwClose(key);
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    status = ZwQueryValueKey(
        key,
        &name,
        KeyValuePartialInformation,
        info,
        size,
        &size
        );
    if (NT_SUCCESS(status) &&
        info->Type == REG_DWORD &&
        info->DataLength >= sizeof(ULONG))
    {
        *value = *(PULONG)info->Data;
    }

    ExFreePoolWithTag(info, 'fKkM');
    ZwClose(key);
    return status;
}
