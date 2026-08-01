/*
 * A1644 HID payload rewrite.
 *
 * Adapted from WinAppleKey (MIT License), Copyright (c) 2021 George Samartzidis.
 * https://github.com/samartzidis/WinAppleKey
 */

#include "mkfilter_a1644.h"

#if !defined(_KERNEL_MODE)
#include <string.h>
#endif

void
MkFilterA1644StateInit(
    MKFILTER_A1644_STATE *state,
    int swapFnCtrl,
    int swapAltCmd
    )
{
    if (state == NULL)
    {
        return;
    }

    state->SwapFnCtrl = swapFnCtrl ? 1 : 0;
    state->SwapAltCmd = swapAltCmd ? 1 : 0;
    state->FakeFnActive = 0;
}

void
MkFilterProcessA1644Payload(
    MK_U8 *buf,
    MK_SIZE size,
    MKFILTER_A1644_STATE *state
    )
{
    MK_U8 *modifier;
    MK_U8 *key1;
    MK_U8 *special;
    int swapFnCtrl;
    int swapAltCmd;

    if (buf == NULL || state == NULL || size != 9)
    {
        return;
    }

    modifier = &buf[0];
    key1 = &buf[2];
    special = &buf[8];
    swapFnCtrl = state->SwapFnCtrl;
    swapAltCmd = state->SwapAltCmd;

    if (swapFnCtrl)
    {
        if ((*modifier & (MK_U8)MkFilterHidLCtrlMask) != 0)
        {
            if (*key1 == 0)
            {
                state->FakeFnActive = 1;
            }
            *modifier = (MK_U8)(*modifier & (MK_U8)~MkFilterHidLCtrlMask);
        }
        else if (*key1 == 0)
        {
            state->FakeFnActive = 0;
        }

        if ((*special & 0x02u) != 0)
        {
            *modifier = (MK_U8)(*modifier | (MK_U8)MkFilterHidLCtrlMask);
        }
        else
        {
            *modifier = (MK_U8)(*modifier & (MK_U8)~MkFilterHidLCtrlMask);
        }
    }
    else
    {
        state->FakeFnActive = ((*special & 0x02u) != 0) ? 1 : 0;
    }

    if ((*special & 0x01u) != 0)
    {
        *key1 = (MK_U8)MkFilterHidDel;
    }
    *special = 0;

    if (swapAltCmd)
    {
        if ((*modifier & (MK_U8)MkFilterHidLAltMask) != 0)
        {
            *modifier = (MK_U8)(
                (*modifier & (MK_U8)~MkFilterHidLAltMask) |
                (MK_U8)MkFilterHidLCmdMask
                );
        }
        else if ((*modifier & (MK_U8)MkFilterHidLCmdMask) != 0)
        {
            *modifier = (MK_U8)(
                (*modifier & (MK_U8)~MkFilterHidLCmdMask) |
                (MK_U8)MkFilterHidLAltMask
                );
        }

        if ((*modifier & (MK_U8)MkFilterHidRAltMask) != 0)
        {
            *modifier = (MK_U8)(
                (*modifier & (MK_U8)~MkFilterHidRAltMask) |
                (MK_U8)MkFilterHidRCmdMask
                );
        }
        else if ((*modifier & (MK_U8)MkFilterHidRCmdMask) != 0)
        {
            *modifier = (MK_U8)(
                (*modifier & (MK_U8)~MkFilterHidRCmdMask) |
                (MK_U8)MkFilterHidRAltMask
                );
        }
    }

    if (state->FakeFnActive != 0 && (*key1 != 0 || *modifier != 0))
    {
        switch (*key1)
        {
        case MkFilterHidLeft:
            *key1 = (MK_U8)MkFilterHidHome;
            break;
        case MkFilterHidRight:
            *key1 = (MK_U8)MkFilterHidEnd;
            break;
        case MkFilterHidUp:
            *key1 = (MK_U8)MkFilterHidPgUp;
            break;
        case MkFilterHidDown:
            *key1 = (MK_U8)MkFilterHidPgDown;
            break;
        case MkFilterHidEnter:
            *key1 = (MK_U8)MkFilterHidInsert;
            break;
        case MkFilterHidF1:
            *key1 = (MK_U8)MkFilterHidF13;
            break;
        case MkFilterHidF2:
            *key1 = (MK_U8)MkFilterHidF14;
            break;
        case MkFilterHidF3:
            *key1 = (MK_U8)MkFilterHidF15;
            break;
        case MkFilterHidF4:
            *key1 = (MK_U8)MkFilterHidF16;
            break;
        case MkFilterHidF5:
            *key1 = (MK_U8)MkFilterHidF17;
            break;
        case MkFilterHidF6:
            *key1 = (MK_U8)MkFilterHidF18;
            break;
        case MkFilterHidF7:
            *key1 = (MK_U8)MkFilterHidF19;
            break;
        case MkFilterHidF8:
            *key1 = (MK_U8)MkFilterHidF20;
            break;
        case MkFilterHidF9:
            *key1 = (MK_U8)MkFilterHidF21;
            break;
        case MkFilterHidF10:
            *key1 = (MK_U8)MkFilterHidF22;
            break;
        case MkFilterHidF11:
            *key1 = (MK_U8)MkFilterHidF23;
            break;
        case MkFilterHidF12:
            *key1 = (MK_U8)MkFilterHidF24;
            break;
        case MkFilterHidKeyP:
            *key1 = (MK_U8)MkFilterHidPrtScr;
            break;
        case MkFilterHidKeyB:
            *key1 = (MK_U8)MkFilterHidPauseBreak;
            break;
        case MkFilterHidKeyS:
            *key1 = (MK_U8)MkFilterHidScrLck;
            break;
        default:
            if ((*modifier & (MK_U8)MkFilterHidLCtrlMask) != 0)
            {
                *modifier = (MK_U8)(
                    (*modifier & (MK_U8)~MkFilterHidLCtrlMask) |
                    (MK_U8)MkFilterHidRCtrlMask
                    );
            }
            else
            {
#if defined(_KERNEL_MODE)
                RtlZeroMemory(buf, size);
#else
                (void)memset(buf, 0, size);
#endif
            }
            break;
        }
    }
}

int
MkFilterHidRewriteEjectToDelete(
    MK_U8 *report,
    MK_SIZE length
    )
{
    MKFILTER_A1644_STATE state;
    MK_U8 before;

    if (report == NULL || length < 10 || report[0] != 0x01)
    {
        return 0;
    }

    before = report[9];
    if ((before & 0x01u) == 0)
    {
        return 0;
    }

    MkFilterA1644StateInit(&state, 0, 0);
    MkFilterProcessA1644Payload(&report[1], 9, &state);
    return 1;
}
