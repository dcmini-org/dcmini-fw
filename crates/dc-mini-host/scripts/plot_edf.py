# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "matplotlib",
#     "mne",
#     "numpy",
# ]
# ///

import mne
import matplotlib.pyplot as plt
import argparse
import numpy as np


def plot_edf(edf_file):
    try:
        # Load the EDF file
        raw = mne.io.read_raw_edf(edf_file, preload=True)
        print(f"\nEDF File Info:")
        print(f"Number of channels: {len(raw.ch_names)}")
        print(f"Channel names: {raw.ch_names}")
        print(f"Sampling frequency: {raw.info['sfreq']} Hz")
        print(f"Number of data points: {raw.n_times}")
        print(f"Duration: {raw.times.max():.2f} seconds")

        # Get data and times
        data, times = raw[:, :]
        # Convert to microvolts
        data *= 1e6

        print(f"{np.min(data)=}, {np.max(data)=}, {np.mean(data)=}, ")

        # Create a figure with subplots for each channel
        n_channels = len(raw.ch_names)
        fig, axes = plt.subplots(
            n_channels, 1, figsize=(15, 2 * n_channels), sharex=True
        )
        fig.suptitle("EEG Channels")

        # Print some basic signal statistics
        print("\nSignal Statistics:")
        for idx, channel_name in enumerate(raw.ch_names):
            print(f"\nChannel {channel_name}:")
            print(f"  Mean: {np.mean(data[idx]):.2f} µV")
            print(f"  Std: {np.std(data[idx]):.2f} µV")
            print(f"  Min: {np.min(data[idx]):.2f} µV")
            print(f"  Max: {np.max(data[idx]):.2f} µV")

        # Plot each channel
        for idx, (ax, channel_name) in enumerate(zip(axes, raw.ch_names)):
            ax.plot(times, data[idx], linewidth=0.5)
            ax.set_ylabel(f"{channel_name}\n(µV)")
            ax.grid(True)

            # Add channel statistics
            mean_val = np.mean(data[idx])
            std_val = np.std(data[idx])
            min_val = np.min(data[idx])
            max_val = np.max(data[idx])
            stats_text = f"mean={mean_val:.1f}, std={std_val:.1f}\nmin={min_val:.1f}, max={max_val:.1f}"
            ax.text(
                0.02,
                0.95,
                stats_text,
                transform=ax.transAxes,
                verticalalignment="top",
                fontsize=8,
            )

        # Add time label to bottom subplot
        axes[-1].set_xlabel("Time (seconds)")

        # Adjust layout and display
        plt.tight_layout()
        plt.show()

    except Exception as e:
        print(f"Error reading EDF file: {str(e)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Plot EEG data from EDF file")
    parser.add_argument("edf_file", help="Path to the EDF file")
    args = parser.parse_args()

    plot_edf(args.edf_file)
