# Android USB Bridge

This folder is where the Unity Android bridge for USB host integration lives.

The native Rust side expects Android to do permission and device opening first, then hand Rust an already-open file descriptor.

## Responsibilities

- enumerate candidate USB devices
- request `UsbManager` permission
- open `UsbDeviceConnection`
- keep the `UsbDeviceConnection` alive while Rust uses the fd
- close the connection on explicit request

## Build note

The Java source tree here is scaffolding. In a production setup this should be built into an `.aar` that Unity consumes from `Assets/Plugins/Android`.

## Current implementation

`DcMiniUsbBridge.java` now supports:

- checking whether Android USB permission already exists for a matching VID/PID
- requesting Android USB permission for a matching VID/PID and tracking the result
- polling the current permission state from Unity C#
- opening the first matching device and returning its file descriptor
- retaining the `UsbDeviceConnection` until `closeFd(...)` is called

The permission flow uses a dynamically registered broadcast receiver tied to `UnityPlayer.currentActivity`.

The Android bridge package/class is `com.dcmini.DcMiniUsbBridge`.
