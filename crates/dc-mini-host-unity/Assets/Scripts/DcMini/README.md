# Unity Scripts

This folder is the Unity-facing half of the binding.

- `Generated/`
  Output path for `csbindgen`. The generated file is derived from the Rust `extern "C"` surface.
- `DcMiniTypes.cs`
  Public Unity-facing DTOs, typed enums, Quest connection options, and reusable poll-buffer helpers.
- `DcMiniClient.cs`
  Handwritten wrapper around the generated native methods with higher-level Quest connection and polling helpers.
- `DcMiniAndroidUsb.cs`
  JNI entrypoint wrapper for the Android USB bridge.
- `DcMiniQuestSample.cs`
  Minimal MonoBehaviour showing Quest/Android permission, USB open, session start, rich ADS polling, and MIC polling flow.

## Intended Unity usage

1. Build `DcMiniQuestUsbOptions` with your VID/PID and desired startup behavior.
2. Call `DcMiniClient.UpdateQuestUsbConnection(...)` from `Update()` until it returns `Connected`.
3. Poll into `DcMiniAdsPollBuffer` and `DcMiniMicPollBuffer`.
4. Read frame data through `DcMiniAdsFrameView` / `DcMiniMicPacketView`.
