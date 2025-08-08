import dc_mini_host_py as dc


def main():
    try:
        # Connect to the device
        client = dc.PyUsbClient()
        print(f"Connected: {client.is_connected()}")

        # Get device information
        device_info = client.get_device_info()
        print(f"Device Info:")
        print(f"  Hardware Version: {device_info.hw_version}")
        print(f"  Firmware Version: {device_info.fw_version}")
        print(f"  Serial Number: {device_info.serial_number}")

        # Get battery level
        battery = client.get_battery_level()
        print("Battery:")
        print(f"  Percentage: {battery.percentage}%")
        print(f"  Voltage: {battery.voltage_mv} mV")
        print(f"  Charging: {battery.charging}")

        # Get current ADS configuration
        config = client.get_ads_config()
        print(f"ADS Configuration: {config=}")

        # Start a recording session
        # session_id = "test_session_001"
        # print(f"Setting session ID to: {session_id}")
        # client.set_session_id(session_id)

        # print("Starting session...")
        # if client.start_session():
        #     print("Session started successfully")
        # else:
        #     print("Failed to start session")

        try:
            # Start streaming data
            def print_data(data):
                print("Received: ", data)

            print("Starting data streaming...")
            streaming_config = client.start_streaming(print_data)
            print(f"Streaming started with sample rate: {streaming_config.sample_rate}")

            # Wait for user input to stop
            input("Press Enter to stop streaming...")

        finally:
            # Stop streaming
            print("Stopping data streaming...")
            client.stop_streaming()

        # Stop session
        # print("Stopping session...")
        # if client.stop_session():
        #     print("Session stopped successfully")
        # else:
        #     print("Failed to stop session")

    except dc.UsbConnectionError as e:
        print(f"Connection error: {e}")
    except dc.UsbCommunicationError as e:
        print(f"Communication error: {e}")
    except Exception as e:
        print(f"Unexpected error: {e}")


if __name__ == "__main__":
    main()
