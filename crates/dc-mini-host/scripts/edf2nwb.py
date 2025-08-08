# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "neuroconv[edf]",
#     "numpy",
#     "tzdata",
# ]
# ///

import argparse
from datetime import datetime
from pathlib import Path
from typing import Optional
from zoneinfo import ZoneInfo
from neuroconv.datainterfaces import EDFRecordingInterface


def convert_edf_to_nwb(
    edf_path: Path,
    nwb_path: Path,
    session_start_time: Optional[datetime] = None,
    timezone: str = "UTC",
    verbose: bool = False,
) -> None:
    """
    Convert an EDF file to NWB format.

    Args:
        edf_path: Path to the input EDF file
        nwb_path: Path to save the output NWB file
        session_start_time: Optional datetime object for the session start time
        timezone: Timezone string (default: "UTC")
        verbose: Whether to print verbose output
    """
    # Create the interface
    interface = EDFRecordingInterface(file_path=str(edf_path), verbose=verbose)

    # Get metadata from the source file
    source_metadata = interface.get_metadata()

    import pprint

    pprint.pprint(dict(source_metadata))

    # If no session start time provided, try to get it from the EDF file
    # or use current time as fallback
    if session_start_time is None:
        try:
            # Try to get the start time from the EDF file
            session_start_time = source_metadata.get("NWBFile", {}).get(
                "session_start_time"
            )
            if session_start_time is None:
                session_start_time = datetime.now()
        except:
            session_start_time = datetime.now()

    # Ensure the session start time has timezone information
    if session_start_time.tzinfo is None:
        session_start_time = session_start_time.replace(tzinfo=ZoneInfo(timezone))

    source_metadata["NWBFile"]["experimenter"] = [
        source_metadata["NWBFile"]["experimenter"]
    ]

    # Create output directory if it doesn't exist
    nwb_path.parent.mkdir(parents=True, exist_ok=True)

    # Run the conversion
    print("Running Conversion")
    interface.run_conversion(nwbfile_path=str(nwb_path), metadata=source_metadata)


def main():
    parser = argparse.ArgumentParser(description="Convert EDF files to NWB format")
    parser.add_argument("input", type=Path, help="Input EDF file path")
    parser.add_argument("output", type=Path, help="Output NWB file path")
    parser.add_argument(
        "--session-start",
        type=lambda s: datetime.fromisoformat(s),
        help="Session start time in ISO format (e.g., 2024-02-07T15:30:00)",
    )
    parser.add_argument(
        "--timezone",
        default="UTC",
        help="Timezone for the session start time (default: UTC)",
    )
    parser.add_argument("--verbose", action="store_true", help="Enable verbose output")

    args = parser.parse_args()

    # Validate input file
    if not args.input.exists():
        parser.error(f"Input file does not exist: {args.input}")
    if args.input.suffix.lower() != ".edf":
        parser.error(f"Input file must be an EDF file: {args.input}")

    try:
        convert_edf_to_nwb(
            edf_path=args.input,
            nwb_path=args.output,
            session_start_time=args.session_start,
            timezone=args.timezone,
            verbose=args.verbose,
        )
        print(f"Successfully converted {args.input} to {args.output}")
    except Exception as e:
        parser.error(f"Conversion failed: {str(e)}")


if __name__ == "__main__":
    main()
