#!/usr/bin/env python3
"""
Example of using the streaming callback in DC Mini Host Python bindings.
This script demonstrates how to receive real-time data from the device.
"""

import time
import numpy as np
import matplotlib.pyplot as plt
from matplotlib.animation import FuncAnimation
import dc_mini_host_py as dc


# Buffer to store the last few seconds of data
class DataBuffer:
    def __init__(self, max_samples=1000, num_channels=8):
        self.max_samples = max_samples
        self.num_channels = num_channels
        self.data = [[] for _ in range(num_channels)]
        self.timestamps = []

    def add_frame(self, frame):
        # Add the new samples to the buffer
        for ch_idx, channel_samples in enumerate(frame.samples):
            if ch_idx < self.num_channels:
                self.data[ch_idx].extend(channel_samples)
                # Trim to max_samples
                if len(self.data[ch_idx]) > self.max_samples:
                    self.data[ch_idx] = self.data[ch_idx][-self.max_samples :]

        # Add timestamps (one per sample)
        num_new_samples = len(frame.samples[0]) if frame.samples else 0
        new_timestamps = [frame.timestamp + i for i in range(num_new_samples)]
        self.timestamps.extend(new_timestamps)

        # Trim timestamps to match data length
        if len(self.timestamps) > self.max_samples:
            self.timestamps = self.timestamps[-self.max_samples :]


# Create a figure for plotting
fig, ax = plt.subplots(figsize=(12, 8))
lines = []
data_buffer = DataBuffer(max_samples=2000, num_channels=8)


# Initialize the plot
def init_plot():
    ax.set_ylim(-1000000, 1000000)  # Adjust based on your signal amplitude
    ax.set_xlim(0, data_buffer.max_samples)
    ax.set_title("DC Mini EEG Data Stream")
    ax.set_xlabel("Sample")
    ax.set_ylabel("Amplitude")

    # Create a line for each channel
    global lines
    for i in range(data_buffer.num_channels):
        (line,) = ax.plot([], [], label=f"Channel {i + 1}")
        lines.append(line)

    ax.legend(loc="upper right")
    return lines


# Update function for the animation
def update_plot(frame):
    # Update each line with the latest data
    for i, line in enumerate(lines):
        if i < len(data_buffer.data) and data_buffer.data[i]:
            # Add an offset to each channel for better visualization
            offset = i * 500000  # Adjust based on your signal amplitude
            y_data = [val + offset for val in data_buffer.data[i]]
            x_data = range(len(y_data))
            line.set_data(x_data, y_data)

    # Adjust x-axis limits if needed
    if data_buffer.data[0]:
        ax.set_xlim(0, len(data_buffer.data[0]))

    return lines


# Callback function for receiving data from the device
def on_data_received(frame):
    # Print some info about the received data
    print(
        f"Received frame with timestamp: {frame.timestamp}, samples: {len(frame.samples)}"
    )
    # data_buffer.add_frame(frame)


def main():
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

        # Start the animation
        ani = FuncAnimation(
            fig, update_plot, init_func=init_plot, interval=100, blit=True
        )

        # Start streaming with our callback
        print("Starting data streaming...")
        config = client.start_streaming(callback=on_data_received)
        print(f"Streaming started with sample rate: {config.sample_rate}")

        # Show the plot (this will block until the window is closed)
        plt.show()

        # Stop streaming when the plot is closed
        print("Stopping data streaming...")
        client.stop_streaming()

    except dc.UsbConnectionError as e:
        print(f"Connection error: {e}")
    except dc.UsbCommunicationError as e:
        print(f"Communication error: {e}")
    except Exception as e:
        print(f"Unexpected error: {e}")
        import traceback

        traceback.print_exc()


if __name__ == "__main__":
    main()
