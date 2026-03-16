# dc-mini-host-unity

Unity-focused native binding crate for DC Mini. This crate is the integration root for:

- the Rust `cdylib` exported surface
- generated `csbindgen` C# bindings
- handwritten Unity wrapper code
- Android USB bridge scaffolding

## Layout

- `src/lib.rs`
  Unity-safe FFI surface. The API uses numeric handles, POD structs, explicit status codes, and polling-based streaming.
- `build.rs`
  Generates `Assets/Scripts/DcMini/Generated/DcMiniNativeMethods.g.cs` from the Rust `extern "C"` surface.
- `Assets/Scripts/DcMini`
  Handwritten Unity wrapper code and asmdef.
- `Assets/Plugins/Android`
  Android manifest and Java bridge for USB host permission/opening.

## Current state

This crate now contains a working Rust-side transport for Android/Linux raw USB file descriptors.

- The Rust ABI is concrete and compileable.
- The Rust transport opens an Android/Linux USB fd with `nusb::Device::from_fd(...)`.
- `postcard-rpc` endpoints and subscriptions are wired for device info, battery, profile, session, DFU, config, ADS/mic streaming, and CVEP decoding events.
- Unity C# bindings are generated into this crate's internal `Assets/` tree.
- The Android bridge handles permission request state and opens a matching device once permission is granted.
- The Unity wrapper owns Android-opened file descriptors when it opens them itself and releases them on `Close()` / `Dispose()`.

## Implemented host parity

The Unity crate now covers the USB-side `dc-mini-host` surface except for BLE:

- device info, battery, connection status, and wait-for-close
- profile get/set/command
- session get/set/start/stop
- ADS config get/set/reset plus per-channel config
- MIC config get/set
- ADS and MIC stream start/stop
- CVEP config/status/start/stop plus decision polling
- DFU begin/write/finish/abort/status plus managed upload helper
- ADS polling in flat or rich mode, with per-sample lead-off/GPIO/IMU metadata
- MIC packet polling

## Intended flow

1. Unity C# asks the Android bridge to obtain USB permission and open a `UsbDeviceConnection`.
2. The Android bridge retains that `UsbDeviceConnection` and returns its raw file descriptor.
3. Rust receives the fd via `dcmini_android_open_usb_fd(...)` and builds the postcard-rpc host transport on top of `nusb`.
4. Rust owns the background runtime, host client, subscriptions, and stream queues.
5. Unity polls ADS and mic buffers each frame or from a dedicated managed thread.

## Unity-facing API shape

The public C# layer is now intentionally higher-level than the raw native ABI:

- typed enums for ADS gain, mux, sample rates, calibration settings, and MIC sample rate
- `DcMiniQuestUsbOptions` and `DcMiniClient.UpdateQuestUsbConnection(...)` for Quest permission/open flow
- `DcMiniAdsPollBuffer` / `DcMiniMicPollBuffer` reusable buffers for low-GC polling
- `DcMiniAdsFrameView` / `DcMiniMicPacketView` helpers for reading flattened poll results

Minimal usage:

```csharp
var client = new DcMiniClient();
var options = DcMiniQuestUsbOptions.Create(vendorId, productId);
options.StartSession = true;
options.StartAdsStream = true;

var ads = new DcMiniAdsPollBuffer(frameCapacity: 8, sampleCapacity: 4096, includeAux: true);

void Update()
{
    var state = client.UpdateQuestUsbConnection(options);
    if (state != DcMiniQuestUsbConnectState.Connected)
    {
        return;
    }

    client.PollAds(ads);
    for (int i = 0; i < (int)ads.FrameCount; i++)
    {
        var frame = ads.GetFrame(i);
        int firstChannel = frame.GetSample(0, 0);
    }
}
```
