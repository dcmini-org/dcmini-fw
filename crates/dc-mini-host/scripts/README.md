# EDF Data Processing Scripts

This directory contains Python scripts for working with EDF (European Data Format) files.

## Available Scripts

### 1. EDF to NWB Converter (`edf2nwb.py`)

Converts EDF files to Neurodata Without Borders (NWB) format.

#### Usage

```bash
uv run edf2nwb.py [options] input output
```

#### Arguments

- `input`: Path to the input EDF file
- `output`: Path for the output NWB file

#### Optional Arguments

- `--metadata METADATA`: JSON file containing additional metadata
- `--session-start SESSION_START`: Session start time in ISO format (e.g., 2024-02-07T15:30:00)
- `--timezone TIMEZONE`: Timezone for the session start time (default: UTC)
- `--verbose`: Enable verbose output

#### Example

```bash
uv run edf2nwb.py input.edf output.nwb --session-start 2024-02-07T15:30:00 --timezone UTC
```

### 2. EDF Plotter (`plot_edf.py`)

Visualizes EEG data from EDF files.

#### Usage

```bash
uv run plot_edf.py edf_file
```

#### Arguments

- `edf_file`: Path to the EDF file to plot

#### Example

```bash
uv run plot_edf.py data.edf
```

## Notes

- All scripts use `uv` as the Python package manager and runner
- Make sure you have the required dependencies installed before running the scripts
- Use the `--help` flag with any script to see detailed usage information

