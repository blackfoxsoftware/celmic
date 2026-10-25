/*++

Celmic virtual microphone - user/kernel contract.

Shared by the driver and documented for user-mode clients (the Tauri app
mirrors these values in apps/desktop/src-tauri/src/vad.rs).

A client opens \\.\CelmicVAD (exclusive, read/write) and periodically calls
IOCTL_CELMIC_PUSH with signed 16-bit little-endian mono PCM at 48 kHz. The
driver queues it in a ring buffer that the capture endpoint drains in real
time. The same call (with or without input) returns a CELMIC_STATUS so the
client can keep the queue near its target fill.

--*/

#ifndef _CELMIC_IOCTL_H_
#define _CELMIC_IOCTL_H_

#define CELMIC_DEVICE_NAME      L"\\Device\\CelmicVAD"
#define CELMIC_SYMBOLIC_NAME    L"\\DosDevices\\CelmicVAD"

#define CELMIC_API_VERSION      1
#define CELMIC_SAMPLE_RATE      48000

// Custom device type (>= 0x8000 is reserved for vendors).
#define FILE_DEVICE_CELMIC      0x0000C31C

// Input: PCM s16le mono 48 kHz (may be empty). Output (optional): CELMIC_STATUS.
#define IOCTL_CELMIC_PUSH \
    CTL_CODE(FILE_DEVICE_CELMIC, 0x800, METHOD_BUFFERED, FILE_WRITE_DATA)

// Drops everything queued (e.g. when the phone disconnects).
#define IOCTL_CELMIC_FLUSH \
    CTL_CODE(FILE_DEVICE_CELMIC, 0x801, METHOD_BUFFERED, FILE_WRITE_DATA)

typedef struct _CELMIC_STATUS
{
    ULONG Version;          // CELMIC_API_VERSION
    ULONG SampleRate;       // CELMIC_SAMPLE_RATE
    ULONG QueuedFrames;     // frames waiting to be captured
    ULONG CapacityFrames;   // ring size
    ULONG ActiveStreams;    // apps currently recording from the mic
    ULONG Overruns;         // frames dropped because the ring was full
    ULONG Underruns;        // capture reads that found the ring empty
    ULONG Reserved;
} CELMIC_STATUS, *PCELMIC_STATUS;

#endif // _CELMIC_IOCTL_H_
