using UnityEngine;

namespace DcMini
{
    public static class DcMiniAndroidUsb
    {
        private const string BridgeClassName = "com.dcmini.DcMiniUsbBridge";

        public static bool HasPermission(int vendorId, int productId)
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            using var bridge = new AndroidJavaClass(BridgeClassName);
            return bridge.CallStatic<bool>("hasPermission", vendorId, productId);
#else
            _ = vendorId;
            _ = productId;
            return false;
#endif
        }

        public static DcMiniAndroidUsbPermissionState GetPermissionState(int vendorId, int productId)
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            using var bridge = new AndroidJavaClass(BridgeClassName);
            return (DcMiniAndroidUsbPermissionState)bridge.CallStatic<int>("getPermissionState", vendorId, productId);
#else
            _ = vendorId;
            _ = productId;
            return DcMiniAndroidUsbPermissionState.Unknown;
#endif
        }

        public static DcMiniAndroidUsbPermissionState RequestPermission(int vendorId, int productId)
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            using var bridge = new AndroidJavaClass(BridgeClassName);
            return (DcMiniAndroidUsbPermissionState)bridge.CallStatic<int>("requestPermission", vendorId, productId);
#else
            _ = vendorId;
            _ = productId;
            return DcMiniAndroidUsbPermissionState.Unknown;
#endif
        }

        public static int OpenFirstMatchingFd(int vendorId, int productId)
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            using var bridge = new AndroidJavaClass(BridgeClassName);
            return bridge.CallStatic<int>("openFirstMatchingFd", vendorId, productId);
#else
            _ = vendorId;
            _ = productId;
            return -1;
#endif
        }

        public static void CloseFd(int fd)
        {
#if UNITY_ANDROID && !UNITY_EDITOR
            using var bridge = new AndroidJavaClass(BridgeClassName);
            bridge.CallStatic("closeFd", fd);
#else
            _ = fd;
#endif
        }
    }
}
