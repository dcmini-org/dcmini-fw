package com.dcmini;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.hardware.usb.UsbDevice;
import android.hardware.usb.UsbDeviceConnection;
import android.hardware.usb.UsbManager;
import android.os.Build;

import com.unity3d.player.UnityPlayer;

import java.util.HashMap;
import java.util.Map;

public final class DcMiniUsbBridge {
    private static final String ACTION_USB_PERMISSION = "com.dcmini.USB_PERMISSION";
    private static final int PERMISSION_UNKNOWN = 0;
    private static final int PERMISSION_NOT_FOUND = 1;
    private static final int PERMISSION_PENDING = 2;
    private static final int PERMISSION_DENIED = 3;
    private static final int PERMISSION_GRANTED = 4;

    private static final Map<Integer, UsbDeviceConnection> openConnections = new HashMap<>();
    private static final Map<String, Integer> permissionStates = new HashMap<>();
    private static BroadcastReceiver permissionReceiver;
    private static boolean permissionReceiverRegistered;

    private DcMiniUsbBridge() {
    }

    public static boolean hasPermission(int vendorId, int productId) {
        return getPermissionState(vendorId, productId) == PERMISSION_GRANTED;
    }

    public static int getPermissionState(int vendorId, int productId) {
        final UsbManager usbManager = getUsbManager();
        if (usbManager == null) {
            return PERMISSION_UNKNOWN;
        }

        final UsbDevice device = findMatchingDevice(usbManager, vendorId, productId);
        if (device == null) {
            permissionStates.put(permissionKey(vendorId, productId), PERMISSION_NOT_FOUND);
            return PERMISSION_NOT_FOUND;
        }

        if (usbManager.hasPermission(device)) {
            permissionStates.put(permissionKey(vendorId, productId), PERMISSION_GRANTED);
            return PERMISSION_GRANTED;
        }

        final Integer current = permissionStates.get(permissionKey(vendorId, productId));
        return current != null ? current : PERMISSION_UNKNOWN;
    }

    public static int requestPermission(int vendorId, int productId) {
        final Activity activity = UnityPlayer.currentActivity;
        final UsbManager usbManager = getUsbManager();
        if (activity == null || usbManager == null) {
            return PERMISSION_UNKNOWN;
        }

        final UsbDevice device = findMatchingDevice(usbManager, vendorId, productId);
        if (device == null) {
            permissionStates.put(permissionKey(vendorId, productId), PERMISSION_NOT_FOUND);
            return PERMISSION_NOT_FOUND;
        }

        if (usbManager.hasPermission(device)) {
            permissionStates.put(permissionKey(vendorId, productId), PERMISSION_GRANTED);
            return PERMISSION_GRANTED;
        }

        ensurePermissionReceiverRegistered(activity);
        final PendingIntent permissionIntent = PendingIntent.getBroadcast(
            activity,
            0,
            new Intent(ACTION_USB_PERMISSION),
            pendingIntentFlags());

        permissionStates.put(permissionKey(vendorId, productId), PERMISSION_PENDING);
        usbManager.requestPermission(device, permissionIntent);
        return PERMISSION_PENDING;
    }

    public static int openFirstMatchingFd(int vendorId, int productId) {
        final UsbManager usbManager = getUsbManager();
        if (usbManager == null) {
            return -1;
        }

        final UsbDevice device = findMatchingDevice(usbManager, vendorId, productId);
        if (device == null || !usbManager.hasPermission(device)) {
            return -1;
        }

        final UsbDeviceConnection connection = usbManager.openDevice(device);
        if (connection == null) {
            return -1;
        }

        final int fd = connection.getFileDescriptor();
        if (fd < 0) {
            connection.close();
            return -1;
        }

        openConnections.put(fd, connection);
        return fd;
    }

    public static void closeFd(int fd) {
        final UsbDeviceConnection connection = openConnections.remove(fd);
        if (connection != null) {
            connection.close();
        }
    }

    private static UsbManager getUsbManager() {
        final Activity activity = UnityPlayer.currentActivity;
        if (activity == null) {
            return null;
        }

        return (UsbManager) activity.getSystemService(Context.USB_SERVICE);
    }

    private static void ensurePermissionReceiverRegistered(Activity activity) {
        if (permissionReceiverRegistered) {
            return;
        }

        permissionReceiver = new BroadcastReceiver() {
            @Override
            public void onReceive(Context context, Intent intent) {
                if (!ACTION_USB_PERMISSION.equals(intent.getAction())) {
                    return;
                }

                final UsbDevice device = intent.getParcelableExtra(UsbManager.EXTRA_DEVICE);
                if (device == null) {
                    return;
                }

                final boolean granted = intent.getBooleanExtra(UsbManager.EXTRA_PERMISSION_GRANTED, false);
                permissionStates.put(
                    permissionKey(device.getVendorId(), device.getProductId()),
                    granted ? PERMISSION_GRANTED : PERMISSION_DENIED);
            }
        };

        final IntentFilter filter = new IntentFilter(ACTION_USB_PERMISSION);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            activity.registerReceiver(permissionReceiver, filter, Context.RECEIVER_NOT_EXPORTED);
        } else {
            activity.registerReceiver(permissionReceiver, filter);
        }

        permissionReceiverRegistered = true;
    }

    private static int pendingIntentFlags() {
        int flags = PendingIntent.FLAG_UPDATE_CURRENT;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            flags |= PendingIntent.FLAG_MUTABLE;
        }

        return flags;
    }

    private static String permissionKey(int vendorId, int productId) {
        return vendorId + ":" + productId;
    }

    private static UsbDevice findMatchingDevice(UsbManager usbManager, int vendorId, int productId) {
        for (UsbDevice device : usbManager.getDeviceList().values()) {
            if (device.getVendorId() == vendorId && device.getProductId() == productId) {
                return device;
            }
        }

        return null;
    }
}
