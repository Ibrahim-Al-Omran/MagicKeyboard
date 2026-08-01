#ifndef MKFILTER_DRIVER_H
#define MKFILTER_DRIVER_H

/*
 * Magic Keyboard A1644 HIDCLASS lower filter (WDM).
 *
 * Architecture adapted from WinAppleKey (MIT License),
 * Copyright (c) 2021 George Samartzidis.
 * https://github.com/samartzidis/WinAppleKey
 *
 * Attaches beneath hidbth/hidusb and rewrites A1644 keyboard payloads on
 * IOCTL_INTERNAL_BTH_SUBMIT_BRB / IOCTL_INTERNAL_USB_SUBMIT_URB completion
 * so Eject (and Fn helpers) become normal HID usages before Windows drops them.
 */

#include <ntddk.h>
#include <wdm.h>
#include <initguid.h>
#include <ntstrsafe.h>
#include <bthdef.h>
#include <ntintsafe.h>
#include <bthguid.h>
#include <bthioctl.h>
#include <bthddi.h>
#include <usbioctl.h>
#include <usbdi.h>

#include "mkfilter_a1644.h"

#ifdef __cplusplus
extern "C" {
#endif

#define MKFILTER_DRIVER_NAME "MagicKeyboardFilter"

#if defined(DBG)
#define MkFilterDebugPrint(s, ...) \
    DbgPrint(MKFILTER_DRIVER_NAME ": " s, __VA_ARGS__)
#else
#define MkFilterDebugPrint(...)
#endif

typedef struct MKFILTER_DEVICE_EXTENSION
{
    PDEVICE_OBJECT DeviceObject;
    PDEVICE_OBJECT LowerDeviceObject;
    PDEVICE_OBJECT Pdo;
    IO_REMOVE_LOCK RemoveLock;
    MKFILTER_A1644_STATE A1644;
} MKFILTER_DEVICE_EXTENSION, *PMKFILTER_DEVICE_EXTENSION;

DRIVER_INITIALIZE DriverEntry;
DRIVER_UNLOAD MkFilterDriverUnload;
DRIVER_ADD_DEVICE MkFilterAddDevice;

_Dispatch_type_(IRP_MJ_PNP)
DRIVER_DISPATCH MkFilterDispatchPnp;

_Dispatch_type_(IRP_MJ_POWER)
DRIVER_DISPATCH MkFilterDispatchPower;

_Dispatch_type_(IRP_MJ_INTERNAL_DEVICE_CONTROL)
DRIVER_DISPATCH MkFilterDispatchInternalIoctl;

DRIVER_DISPATCH MkFilterDispatchAny;

void
MkFilterRemoveDevice(
    PDEVICE_OBJECT fido
    );

ULONG
MkFilterGetLowerDeviceType(
    PDEVICE_OBJECT pdo
    );

NTSTATUS
MkFilterCompleteRequest(
    PIRP irp,
    NTSTATUS status,
    ULONG_PTR info
    );

NTSTATUS
MkFilterReadRegistryDword(
    PUNICODE_STRING registryPath,
    PCWSTR valueName,
    PULONG value
    );

#ifdef __cplusplus
}
#endif

#endif
