#!/usr/bin/env python3
"""
Example of using Rerun to visualize streaming data from DC Mini Host.
This script demonstrates real-time visualization of EEG data with advanced Rerun features.
"""

import time
import numpy as np
import rerun as rr
import rerun.blueprint as rrb
import dc_mini_host_py as dc
from datetime import datetime


# Buffer to store recent data for display
class DataBuffer:
    def __init__(self, max_samples=1000, num_channels=8, window_size=5.0):
        self.max_samples = max_samples
        self.num_channels = num_channels
        self.data = [[] for _ in range(num_channels)]
        self.timestamps = []
        self.latest_timestamp = 0
        self.window_size = window_size  # Window size in seconds
        self.initialized = False

    def add_frame(self, frame):
        self.latest_timestamp = frame.timestamp

        # Use the channel_data field which is already organized by channel
        for ch_idx, channel_data in enumerate(frame.channel_data):
            if ch_idx < self.num_channels:
                self.data[ch_idx].extend(channel_data)
                # Trim to max_samples
                if len(self.data[ch_idx]) > self.max_samples:
                    self.data[ch_idx] = self.data[ch_idx][-self.max_samples :]

        # Add timestamps (convert device timestamp to seconds)
        num_new_samples = (
            len(frame.channel_data[0])
            if frame.channel_data and frame.channel_data[0]
            else 0
        )
        if num_new_samples > 0:
            # Convert device timestamp to seconds for Rerun
            device_time_sec = (
                frame.timestamp / 1000.0
            )  # Assuming timestamp is in milliseconds

            # Create evenly spaced timestamps for the samples
            sample_interval = (
                1.0 / 250.0
            )  # Assuming 250Hz sample rate, adjust as needed
            new_timestamps = np.linspace(
                device_time_sec,
                device_time_sec + (num_new_samples - 1) * sample_interval,
                num_new_samples,
            )
            self.timestamps.extend(new_timestamps)

            # Trim timestamps
            if len(self.timestamps) > self.max_samples:
                self.timestamps = self.timestamps[-self.max_samples :]


# Create a data buffer
data_buffer = DataBuffer(max_samples=5000, num_channels=8, window_size=10.0)


# Callback function for receiving data from the device
def on_data_received(frame):
    # Add data to our buffer
    data_buffer.add_frame(frame)

    # Log the data to Rerun
    log_data_to_rerun()

    # Print some info about the received data (less frequently to avoid console spam)
    if frame.timestamp % 1000 < 10:  # Print roughly every second
        print(
            f"Received frame with timestamp: {frame.timestamp}, samples: {len(frame.samples)}"
        )


def setup_blueprint():
    """Set up the Rerun blueprint for visualization"""
    # Create a grid of time series views for each channel
    grid_contents = [
        rrb.TimeSeriesView(
            origin=f"eeg/channel_{i + 1}",
            name=f"Channel {i + 1}",
            time_ranges=[
                rrb.VisibleTimeRange(
                    "time",
                    start=rrb.TimeRangeBoundary.cursor_relative(
                        seconds=-data_buffer.window_size
                    ),
                    end=rrb.TimeRangeBoundary.cursor_relative(),
                )
            ],
            plot_legend=rrb.PlotLegend(visible=False),
        )
        for i in range(data_buffer.num_channels)
    ]

    # Add a heatmap view
    heatmap_view = rrb.TensorView(
        origin="eeg/heatmap",
        name="Channel Heatmap",
    )

    # Add text views for device info and config
    info_view = rrb.TextDocumentView(
        origin="device/info",
        name="Device Info",
    )

    config_view = rrb.TextDocumentView(
        origin="device/config",
        name="Configuration",
    )

    # Create the main layout
    main_layout = rrb.Horizontal(
        contents=[
            rrb.Vertical(
                contents=[
                    rrb.Horizontal(
                        contents=[info_view, config_view],
                        column_shares=[1, 1],
                    ),
                    heatmap_view,
                ],
                row_shares=[1, 3],
            ),
            rrb.Grid(contents=grid_contents),
        ],
        column_shares=[1, 2],
    )

    # Send the blueprint to Rerun
    rr.send_blueprint(main_layout)
    print("Blueprint Initialized!")
    data_buffer.initialized = True


def log_data_to_rerun():
    """Log the current data buffer to Rerun"""
    # Initialize the blueprint if not already done
    if not data_buffer.initialized:
        setup_blueprint()

    # Log each channel as a separate time series
    for ch_idx, channel_data in enumerate(data_buffer.data):
        if channel_data and data_buffer.timestamps:
            # Get the most recent data that matches our timestamps
            n_samples = min(len(channel_data), len(data_buffer.timestamps))
            if n_samples > 0:
                times = np.array(data_buffer.timestamps[-n_samples:])
                values = np.array(channel_data[-n_samples:])

                # Send to Rerun using columns for better performance
                rr.send_columns(
                    f"eeg/channel_{ch_idx + 1}",
                    indexes=[rr.TimeSecondsColumn("time", times)],
                    columns=[rr.components.ScalarBatch(values)],
                )

    # Log a heatmap of all channels
    if all(len(ch) > 0 for ch in data_buffer.data):
        # Create a 2D array with channels as rows
        min_len = min(len(ch) for ch in data_buffer.data)
        if min_len > 0:
            heatmap_data = np.vstack([ch[-min_len:] for ch in data_buffer.data])

            # Normalize for better visualization
            heatmap_data = (heatmap_data - np.mean(heatmap_data)) / (
                np.std(heatmap_data) + 1e-6
            )

            # Log the heatmap
            rr.log(
                "eeg/heatmap",
                rr.Tensor2D(
                    heatmap_data,
                    colormap=rr.ColorMap.VIRIDIS,
                ),
            )


def main():
    # Initialize Rerun
    rr.init("DC Mini EEG Stream", spawn=True)

    try:
        # Connect to the device
        client = dc.PyUsbClient()
        if not client.is_connected():
            print("Failed to connect to device")
            return

        print("Connected to device")

        # Get device info
        device_info = client.get_device_info()
        print(f"Device: {device_info.hw_version} / {device_info.fw_version}")

        # Log device info to Rerun
        rr.log(
            "device/info",
            rr.TextDocument(
                f"Hardware: {device_info.hw_version}\n"
                f"Firmware: {device_info.fw_version}\n"
                f"Serial: {device_info.serial_number}\n"
                f"Connected: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}"
            ),
        )

        # Start streaming with our callback
        print("Starting data streaming...")
        config = client.start_streaming(callback=on_data_received)
        print(f"Streaming started with sample rate: {config.sample_rate}")

        # Log config to Rerun
        rr.log("device/config", rr.TextDocument(f"Sample Rate: {config.sample_rate}\n"))

        # Keep the script running until Ctrl+C
        try:
            print("Streaming data to Rerun viewer. Press Ctrl+C to stop...")
            while True:
                time.sleep(0.1)
        except KeyboardInterrupt:
            print("\nStopping streaming...")

        # Stop streaming
        client.stop_streaming()

    except dc.UsbConnectionError as e:
        print(f"Connection error: {e}")
        rr.log("error", rr.TextDocument(f"Connection error: {e}"))
    except dc.UsbCommunicationError as e:
        print(f"Communication error: {e}")
        rr.log("error", rr.TextDocument(f"Communication error: {e}"))
    except Exception as e:
        print(f"Unexpected error: {e}")
        import traceback

        error_text = traceback.format_exc()
        print(error_text)
        rr.log("error", rr.TextDocument(f"Unexpected error:\n{error_text}"))


if __name__ == "__main__":
    main()
