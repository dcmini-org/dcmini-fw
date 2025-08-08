#!/usr/bin/env python3
"""
Test script for DC Mini Host Python bindings.
This script tests the basic functionality of the Python bindings.
"""

import sys
import time
import dc_mini_host_py as dc


def test_connection():
    """Test basic connection to the device"""
    print("Testing connection...")
    try:
        client = dc.PyUsbClient()
        if client.is_connected():
            print("✅ Connection successful")
            return client
        else:
            print("❌ Device connected but reports as disconnected")
            return None
    except dc.UsbConnectionError as e:
        print(f"❌ Connection failed: {e}")
        return None
    except Exception as e:
        print(f"❌ Unexpected error: {e}")
        return None


def test_device_info(client):
    """Test retrieving device information"""
    print("\nTesting device info retrieval...")
    try:
        info = client.get_device_info()
        print("✅ Device info retrieved successfully:")
        print(f"  • Hardware Version: {info.hw_version}")
        print(f"  • Firmware Version: {info.fw_version}")
        print(f"  • Serial Number: {info.serial_number}")
        return True
    except Exception as e:
        print(f"❌ Failed to get device info: {e}")
        return False


def test_battery_level(client):
    """Test retrieving battery level"""
    print("\nTesting battery level retrieval...")
    try:
        battery = client.get_battery_level()
        print("✅ Battery level retrieved successfully:")
        print(f"  • Percentage: {battery.percentage}%")
        print(f"  • Voltage: {battery.voltage_mv} mV")
        print(f"  • Charging: {battery.charging}")
        return True
    except Exception as e:
        print(f"❌ Failed to get battery level: {e}")
        return False


def test_ads_config(client):
    """Test retrieving and setting ADS configuration"""
    print("\nTesting ADS configuration...")
    try:
        # Get current config
        config = client.get_ads_config()
        print("✅ ADS config retrieved successfully:")
        print(f"  • Sample Rate: {config.sample_rate}")
        print(f"  • Gain: {config.gain}")

        # Try to set the same config back
        result = client.set_ads_config(config)
        if result:
            print("✅ ADS config set successfully")
        else:
            print("❌ Failed to set ADS config")

        return True
    except Exception as e:
        print(f"❌ Failed to work with ADS config: {e}")
        return False


def test_session(client):
    """Test session operations"""
    print("\nTesting session operations...")
    try:
        # Get current session status
        status = client.get_session_status()
        print(f"  • Initial session status: {'Active' if status else 'Inactive'}")

        # Set session ID
        test_id = f"test_{int(time.time())}"
        if client.set_session_id(test_id):
            print(f"✅ Session ID set to: {test_id}")
        else:
            print("❌ Failed to set session ID")
            return False

        # Get session ID to verify
        retrieved_id = client.get_session_id()
        if retrieved_id == test_id:
            print(f"✅ Session ID verified: {retrieved_id}")
        else:
            print(f"❌ Session ID mismatch: expected {test_id}, got {retrieved_id}")
            return False

        return True
    except Exception as e:
        print(f"❌ Failed to work with session: {e}")
        return False


def test_streaming(client):
    """Test data streaming (brief test)"""
    print("\nTesting data streaming (brief test)...")
    try:
        # Start streaming
        config = client.start_streaming()
        print(f"✅ Streaming started with sample rate: {config.sample_rate}")

        # Wait briefly
        print("  • Streaming for 2 seconds...")
        time.sleep(2)

        # Stop streaming
        client.stop_streaming()
        print("✅ Streaming stopped successfully")

        return True
    except Exception as e:
        print(f"❌ Streaming test failed: {e}")
        return False


def main():
    """Main test function"""
    print("DC Mini Host Python Bindings Test\n" + "=" * 35)

    # Test connection
    client = test_connection()
    if not client:
        print("\n❌ Test failed: Could not connect to device")
        sys.exit(1)

    # Run all tests
    tests = [
        test_device_info,
        test_battery_level,
        test_ads_config,
        test_session,
        test_streaming,
    ]

    results = []
    for test in tests:
        results.append(test(client))

    # Print summary
    print("\nTest Summary:")
    print(f"  • Tests passed: {results.count(True)}/{len(results)}")
    print(f"  • Tests failed: {results.count(False)}/{len(results)}")

    if False in results:
        print("\n❌ Some tests failed")
        sys.exit(1)
    else:
        print("\n✅ All tests passed successfully!")
        sys.exit(0)


if __name__ == "__main__":
    main()
