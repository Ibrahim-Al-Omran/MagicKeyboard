#ifndef MKFILTER_A1644_H
#define MKFILTER_A1644_H

/*
 * A1644 HID payload rewrite.
 *
 * Adapted from WinAppleKey (MIT License), Copyright (c) 2021 George Samartzidis.
 * https://github.com/samartzidis/WinAppleKey
 *
 * Wire payload (9 bytes, no report ID):
 *   [0] modifiers
 *   [1] reserved / Apple vendor byte
 *   [2..7] six HID keycodes
 *   [8] special bits: bit0 = Eject, bit1 = physical Fn
 */

#if defined(_KERNEL_MODE)
#include <ntddk.h>
typedef UCHAR MK_U8;
typedef SIZE_T MK_SIZE;
#else
#include <stddef.h>
#include <stdint.h>
typedef uint8_t MK_U8;
typedef size_t MK_SIZE;
#endif

#ifdef __cplusplus
extern "C" {
#endif

enum MkFilterHidCodes
{
    MkFilterHidDel = 0x4c,
    MkFilterHidLeft = 0x50,
    MkFilterHidHome = 0x4a,
    MkFilterHidRight = 0x4f,
    MkFilterHidEnd = 0x4d,
    MkFilterHidUp = 0x52,
    MkFilterHidPgUp = 0x4b,
    MkFilterHidDown = 0x51,
    MkFilterHidPgDown = 0x4e,
    MkFilterHidEnter = 0x28,
    MkFilterHidInsert = 0x49,
    MkFilterHidPrtScr = 0x46,
    MkFilterHidScrLck = 0x47,
    MkFilterHidPauseBreak = 0x48,
    MkFilterHidF1 = 0x3a,
    MkFilterHidF2 = 0x3b,
    MkFilterHidF3 = 0x3c,
    MkFilterHidF4 = 0x3d,
    MkFilterHidF5 = 0x3e,
    MkFilterHidF6 = 0x3f,
    MkFilterHidF7 = 0x40,
    MkFilterHidF8 = 0x41,
    MkFilterHidF9 = 0x42,
    MkFilterHidF10 = 0x43,
    MkFilterHidF11 = 0x44,
    MkFilterHidF12 = 0x45,
    MkFilterHidF13 = 0x68,
    MkFilterHidF14 = 0x69,
    MkFilterHidF15 = 0x6a,
    MkFilterHidF16 = 0x6b,
    MkFilterHidF17 = 0x6c,
    MkFilterHidF18 = 0x6d,
    MkFilterHidF19 = 0x6e,
    MkFilterHidF20 = 0x6f,
    MkFilterHidF21 = 0x70,
    MkFilterHidF22 = 0x71,
    MkFilterHidF23 = 0x72,
    MkFilterHidF24 = 0x73,
    MkFilterHidKeyB = 0x05,
    MkFilterHidKeyP = 0x13,
    MkFilterHidKeyS = 0x16,
    MkFilterHidLCtrlMask = 0x01,
    MkFilterHidRCtrlMask = 0x10,
    MkFilterHidLAltMask = 0x04,
    MkFilterHidRAltMask = 0x40,
    MkFilterHidLCmdMask = 0x08,
    MkFilterHidRCmdMask = 0x80
};

typedef struct MKFILTER_A1644_STATE
{
    int SwapFnCtrl;
    int SwapAltCmd;
    int FakeFnActive;
} MKFILTER_A1644_STATE;

void
MkFilterA1644StateInit(
    MKFILTER_A1644_STATE *state,
    int swapFnCtrl,
    int swapAltCmd
    );

void
MkFilterProcessA1644Payload(
    MK_U8 *buf,
    MK_SIZE size,
    MKFILTER_A1644_STATE *state
    );

int
MkFilterHidRewriteEjectToDelete(
    MK_U8 *report,
    MK_SIZE length
    );

#ifdef __cplusplus
}
#endif

#endif
