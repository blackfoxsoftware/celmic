/*++

Celmic virtual microphone - control device and audio ring buffer.

--*/

#ifndef _CELMIC_H_
#define _CELMIC_H_

#include "celmic_ioctl.h"

// DriverEntry, after PcInitializeAdapterDriver: allocates the ring and hooks
// the dispatch table so IRPs for our control device bypass PortCls.
_IRQL_requires_(PASSIVE_LEVEL)
NTSTATUS CelmicDriverInit(_In_ PDRIVER_OBJECT DriverObject);

// DriverUnload: frees the ring.
_IRQL_requires_(PASSIVE_LEVEL)
VOID CelmicDriverUnload();

// StartDevice / REMOVE_DEVICE: lifetime of \\.\CelmicVAD.
_IRQL_requires_(PASSIVE_LEVEL)
NTSTATUS CelmicCreateControlDevice(_In_ PDRIVER_OBJECT DriverObject);

_IRQL_requires_(PASSIVE_LEVEL)
VOID CelmicDeleteControlDevice();

// Capture stream: fills `Bytes` of `Buffer` (in the stream's format) from the ring.
_IRQL_requires_max_(DISPATCH_LEVEL)
VOID CelmicReadCapture(_Out_writes_bytes_(Bytes) BYTE* Buffer, _In_ ULONG Bytes, _In_ const WAVEFORMATEX* Format);

_IRQL_requires_max_(DISPATCH_LEVEL)
VOID CelmicStreamStarted();

_IRQL_requires_max_(DISPATCH_LEVEL)
VOID CelmicStreamStopped();

#endif // _CELMIC_H_
