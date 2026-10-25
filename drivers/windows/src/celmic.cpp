/*++

Celmic virtual microphone - control device and audio ring buffer.

The PortCls adapter owns the driver's dispatch table. We save its routines,
install our own, and route IRPs that target our control device object
(\Device\CelmicVAD) here; everything else goes straight back to PortCls.

The control device is identified by its device type *and* a magic value in
its extension rather than by a global pointer, so IRPs that arrive on a
handle opened before the device was deleted are still recognized and never
reach PortCls with a foreign device object.

--*/

#include "definitions.h"
#include <wdmsec.h>
#include "celmic.h"

#define CELMIC_POOLTAG          'cimC'
#define CELMIC_CDO_MAGIC        0x434D4943  // "CMIC"
#define CELMIC_RING_FRAMES      CELMIC_SAMPLE_RATE              // 1 s of audio
#define CELMIC_MAX_PUSH_BYTES   (CELMIC_SAMPLE_RATE * sizeof(SHORT))

// {9b0f1c1e-4f3c-4c5b-9a57-1d2b6f6c43a1}
static const GUID GUID_CELMIC_CONTROL_CLASS =
    { 0x9b0f1c1e, 0x4f3c, 0x4c5b, { 0x9a, 0x57, 0x1d, 0x2b, 0x6f, 0x6c, 0x43, 0xa1 } };

typedef struct _CELMIC_CDO_EXTENSION
{
    ULONG Magic;
} CELMIC_CDO_EXTENSION, *PCELMIC_CDO_EXTENSION;

typedef struct _CELMIC_RING
{
    KSPIN_LOCK  Lock;
    SHORT*      Samples;
    ULONG       Capacity;
    ULONG       Read;
    ULONG       Count;
    ULONG       Overruns;
    ULONG       Underruns;
} CELMIC_RING;

static PDRIVER_DISPATCH g_PortClsDispatch[IRP_MJ_MAXIMUM_FUNCTION + 1];
static PDEVICE_OBJECT   g_ControlDevice = NULL;
static CELMIC_RING      g_Ring;
static volatile LONG    g_ActiveStreams = 0;

_Dispatch_type_(IRP_MJ_CREATE)
_Dispatch_type_(IRP_MJ_CLOSE)
_Dispatch_type_(IRP_MJ_CLEANUP)
_Dispatch_type_(IRP_MJ_DEVICE_CONTROL)
static DRIVER_DISPATCH CelmicDispatch;

//=============================================================================
// Ring buffer
//=============================================================================

#pragma code_seg()
static BOOLEAN IsControlDevice(_In_ PDEVICE_OBJECT DeviceObject)
{
    return DeviceObject != NULL &&
           DeviceObject->DeviceType == FILE_DEVICE_CELMIC &&
           DeviceObject->DeviceExtension != NULL &&
           static_cast<PCELMIC_CDO_EXTENSION>(DeviceObject->DeviceExtension)->Magic == CELMIC_CDO_MAGIC;
}

#pragma code_seg()
static VOID RingWrite(_In_reads_(Count) const SHORT* Source, _In_ ULONG Count)
{
    KIRQL irql;
    KeAcquireSpinLock(&g_Ring.Lock, &irql);

    // More than fits: keep only the newest audio.
    if (Count > g_Ring.Capacity)
    {
        g_Ring.Overruns += Count - g_Ring.Capacity;
        Source += Count - g_Ring.Capacity;
        Count = g_Ring.Capacity;
    }

    // Make room by dropping the oldest frames.
    if (g_Ring.Count + Count > g_Ring.Capacity)
    {
        ULONG drop = g_Ring.Count + Count - g_Ring.Capacity;
        g_Ring.Read = (g_Ring.Read + drop) % g_Ring.Capacity;
        g_Ring.Count -= drop;
        g_Ring.Overruns += drop;
    }

    ULONG write = (g_Ring.Read + g_Ring.Count) % g_Ring.Capacity;
    ULONG first = min(Count, g_Ring.Capacity - write);
    RtlCopyMemory(g_Ring.Samples + write, Source, first * sizeof(SHORT));
    RtlCopyMemory(g_Ring.Samples, Source + first, (Count - first) * sizeof(SHORT));
    g_Ring.Count += Count;

    KeReleaseSpinLock(&g_Ring.Lock, irql);
}

#pragma code_seg()
static VOID RingFlush()
{
    KIRQL irql;
    KeAcquireSpinLock(&g_Ring.Lock, &irql);
    g_Ring.Read = 0;
    g_Ring.Count = 0;
    KeReleaseSpinLock(&g_Ring.Lock, irql);
}

#pragma code_seg()
static VOID FillStatus(_Out_ PCELMIC_STATUS Status)
{
    RtlZeroMemory(Status, sizeof(*Status));
    Status->Version = CELMIC_API_VERSION;
    Status->SampleRate = CELMIC_SAMPLE_RATE;
    Status->CapacityFrames = g_Ring.Capacity;
    Status->ActiveStreams = (ULONG)InterlockedCompareExchange(&g_ActiveStreams, 0, 0);

    KIRQL irql;
    KeAcquireSpinLock(&g_Ring.Lock, &irql);
    Status->QueuedFrames = g_Ring.Count;
    Status->Overruns = g_Ring.Overruns;
    Status->Underruns = g_Ring.Underruns;
    KeReleaseSpinLock(&g_Ring.Lock, irql);
}

//=============================================================================
// Capture side (called by CMiniportWaveRTStream under its position spinlock)
//=============================================================================

#pragma code_seg()
static __forceinline VOID WriteSample(_Out_writes_bytes_(ContainerBytes) BYTE* Target, _In_ ULONG ContainerBytes, _In_ SHORT Sample)
{
    switch (ContainerBytes)
    {
    case 2:
        *reinterpret_cast<SHORT*>(Target) = Sample;
        break;
    case 3:
    {
        LONG value = static_cast<LONG>(Sample) << 8;
        Target[0] = static_cast<BYTE>(value);
        Target[1] = static_cast<BYTE>(value >> 8);
        Target[2] = static_cast<BYTE>(value >> 16);
        break;
    }
    case 4:
        // Integer PCM, MSB-aligned (also correct for 24-in-32 containers).
        *reinterpret_cast<LONG*>(Target) = static_cast<LONG>(Sample) << 16;
        break;
    default:
        RtlZeroMemory(Target, ContainerBytes);
        break;
    }
}

#pragma code_seg()
VOID CelmicReadCapture(_Out_writes_bytes_(Bytes) BYTE* Buffer, _In_ ULONG Bytes, _In_ const WAVEFORMATEX* Format)
{
    ULONG blockAlign = Format->nBlockAlign;
    ULONG channels = Format->nChannels;

    if (Buffer == NULL || Bytes == 0)
    {
        return;
    }
    if (blockAlign == 0 || channels == 0 || g_Ring.Samples == NULL)
    {
        RtlZeroMemory(Buffer, Bytes);
        return;
    }

    ULONG frames = Bytes / blockAlign;
    ULONG container = blockAlign / channels;

    KIRQL irql;
    KeAcquireSpinLock(&g_Ring.Lock, &irql);

    ULONG available = min(frames, g_Ring.Count);
    if (available < frames)
    {
        g_Ring.Underruns++;
    }

    BYTE* out = Buffer;
    for (ULONG i = 0; i < frames; i++)
    {
        SHORT sample = 0;
        if (i < available)
        {
            sample = g_Ring.Samples[(g_Ring.Read + i) % g_Ring.Capacity];
        }
        for (ULONG c = 0; c < channels; c++)
        {
            WriteSample(out, container, sample);
            out += container;
        }
    }

    g_Ring.Read = (g_Ring.Read + available) % g_Ring.Capacity;
    g_Ring.Count -= available;

    KeReleaseSpinLock(&g_Ring.Lock, irql);

    // Partial trailing frame (should not happen with whole-block displacements).
    ULONG tail = Bytes - frames * blockAlign;
    if (tail > 0)
    {
        RtlZeroMemory(out, tail);
    }
}

#pragma code_seg()
VOID CelmicStreamStarted()
{
    InterlockedIncrement(&g_ActiveStreams);
}

#pragma code_seg()
VOID CelmicStreamStopped()
{
    if (InterlockedDecrement(&g_ActiveStreams) < 0)
    {
        InterlockedExchange(&g_ActiveStreams, 0);
    }
}

//=============================================================================
// Control device
//=============================================================================

#pragma code_seg()
static NTSTATUS CompleteIrp(_Inout_ PIRP Irp, _In_ NTSTATUS Status, _In_ ULONG_PTR Information)
{
    Irp->IoStatus.Status = Status;
    Irp->IoStatus.Information = Information;
    IoCompleteRequest(Irp, IO_NO_INCREMENT);
    return Status;
}

#pragma code_seg()
static NTSTATUS HandleDeviceControl(_Inout_ PIRP Irp, _In_ PIO_STACK_LOCATION Stack)
{
    ULONG code = Stack->Parameters.DeviceIoControl.IoControlCode;
    ULONG inLength = Stack->Parameters.DeviceIoControl.InputBufferLength;
    ULONG outLength = Stack->Parameters.DeviceIoControl.OutputBufferLength;
    PVOID buffer = Irp->AssociatedIrp.SystemBuffer;

    switch (code)
    {
    case IOCTL_CELMIC_PUSH:
        if (inLength % sizeof(SHORT) != 0 || inLength > CELMIC_MAX_PUSH_BYTES ||
            (inLength > 0 && buffer == NULL))
        {
            return CompleteIrp(Irp, STATUS_INVALID_PARAMETER, 0);
        }
        if (inLength > 0)
        {
            RingWrite(static_cast<const SHORT*>(buffer), inLength / sizeof(SHORT));
        }
        break;

    case IOCTL_CELMIC_FLUSH:
        RingFlush();
        break;

    default:
        return CompleteIrp(Irp, STATUS_INVALID_DEVICE_REQUEST, 0);
    }

    // METHOD_BUFFERED: the system buffer is max(in, out) bytes, so the status
    // can be written over the (already consumed) input.
    if (outLength == 0)
    {
        return CompleteIrp(Irp, STATUS_SUCCESS, 0);
    }
    if (outLength < sizeof(CELMIC_STATUS) || buffer == NULL)
    {
        return CompleteIrp(Irp, STATUS_BUFFER_TOO_SMALL, 0);
    }
    FillStatus(static_cast<PCELMIC_STATUS>(buffer));
    return CompleteIrp(Irp, STATUS_SUCCESS, sizeof(CELMIC_STATUS));
}

#pragma code_seg()
static NTSTATUS CelmicDispatch(_In_ PDEVICE_OBJECT DeviceObject, _Inout_ PIRP Irp)
{
    PIO_STACK_LOCATION stack = IoGetCurrentIrpStackLocation(Irp);

    if (!IsControlDevice(DeviceObject))
    {
        return g_PortClsDispatch[stack->MajorFunction](DeviceObject, Irp);
    }

    switch (stack->MajorFunction)
    {
    case IRP_MJ_CREATE:
    case IRP_MJ_CLOSE:
        return CompleteIrp(Irp, STATUS_SUCCESS, 0);

    case IRP_MJ_CLEANUP:
        // The feeding app went away: don't let stale audio play later.
        RingFlush();
        return CompleteIrp(Irp, STATUS_SUCCESS, 0);

    case IRP_MJ_DEVICE_CONTROL:
        return HandleDeviceControl(Irp, stack);

    default:
        return CompleteIrp(Irp, STATUS_INVALID_DEVICE_REQUEST, 0);
    }
}

#pragma code_seg("PAGE")
NTSTATUS CelmicDriverInit(_In_ PDRIVER_OBJECT DriverObject)
{
    PAGED_CODE();

    KeInitializeSpinLock(&g_Ring.Lock);
    g_Ring.Capacity = CELMIC_RING_FRAMES;
    g_Ring.Samples = static_cast<SHORT*>(
        ExAllocatePool2(POOL_FLAG_NON_PAGED, g_Ring.Capacity * sizeof(SHORT), CELMIC_POOLTAG));
    if (g_Ring.Samples == NULL)
    {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    // Every major function goes through CelmicDispatch so no IRP for the
    // control device can ever be interpreted by PortCls.
    for (ULONG i = 0; i <= IRP_MJ_MAXIMUM_FUNCTION; i++)
    {
        g_PortClsDispatch[i] = DriverObject->MajorFunction[i];
        DriverObject->MajorFunction[i] = CelmicDispatch;
    }

    return STATUS_SUCCESS;
}

#pragma code_seg("PAGE")
VOID CelmicDriverUnload()
{
    PAGED_CODE();

    if (g_Ring.Samples != NULL)
    {
        ExFreePoolWithTag(g_Ring.Samples, CELMIC_POOLTAG);
        g_Ring.Samples = NULL;
    }
}

#pragma code_seg("PAGE")
NTSTATUS CelmicCreateControlDevice(_In_ PDRIVER_OBJECT DriverObject)
{
    PAGED_CODE();

    if (g_ControlDevice != NULL)
    {
        return STATUS_SUCCESS;
    }

    UNICODE_STRING deviceName = RTL_CONSTANT_STRING(CELMIC_DEVICE_NAME);
    UNICODE_STRING linkName = RTL_CONSTANT_STRING(CELMIC_SYMBOLIC_NAME);
    PDEVICE_OBJECT device = NULL;

    // Any local user may feed the microphone; only one handle at a time.
    NTSTATUS status = IoCreateDeviceSecure(
        DriverObject,
        sizeof(CELMIC_CDO_EXTENSION),
        &deviceName,
        FILE_DEVICE_CELMIC,
        FILE_DEVICE_SECURE_OPEN,
        TRUE,
        &SDDL_DEVOBJ_SYS_ALL_ADM_RWX_WORLD_RW_RES_R,
        &GUID_CELMIC_CONTROL_CLASS,
        &device);
    if (!NT_SUCCESS(status))
    {
        return status;
    }

    static_cast<PCELMIC_CDO_EXTENSION>(device->DeviceExtension)->Magic = CELMIC_CDO_MAGIC;

    status = IoCreateSymbolicLink(&linkName, &deviceName);
    if (!NT_SUCCESS(status))
    {
        IoDeleteDevice(device);
        return status;
    }

    device->Flags &= ~DO_DEVICE_INITIALIZING;
    g_ControlDevice = device;
    return STATUS_SUCCESS;
}

#pragma code_seg("PAGE")
VOID CelmicDeleteControlDevice()
{
    PAGED_CODE();

    if (g_ControlDevice == NULL)
    {
        return;
    }

    UNICODE_STRING linkName = RTL_CONSTANT_STRING(CELMIC_SYMBOLIC_NAME);
    IoDeleteSymbolicLink(&linkName);
    IoDeleteDevice(g_ControlDevice);
    g_ControlDevice = NULL;
}

#pragma code_seg()
