from __future__ import annotations

from cvep_bench.datasets.models import BenchmarkProfile, PreprocessingOptions


PRETRIAL_BUFFER_SECONDS = 0.5
THIELEN2021_KEY_WINDOWS = (1.05, 2.1, 4.2, 5.25, 10.5, 31.5)

BENCHMARK_PROFILES: dict[str, BenchmarkProfile] = {
    "legacy": BenchmarkProfile(
        name="legacy",
        description="Current default benchmark settings.",
        target_fs=250,
        band_low=1.0,
        band_high=65.0,
        notch_hz=50.0,
        drop_first_seconds=0.0,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        default_window_seconds_grid=None,
    ),
    "matched_embedded_125": BenchmarkProfile(
        name="matched_embedded_125",
        description="Embedded-relevant 125 Hz comparison profile.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.0,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
    "matched_diagnostic_125": BenchmarkProfile(
        name="matched_diagnostic_125",
        description="Embedded 125 Hz profile with first 500 ms removed.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.5,
        event="refe",
        onset_event=False,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
    "matched_onset_aware_125": BenchmarkProfile(
        name="matched_onset_aware_125",
        description="Embedded 125 Hz profile with onset-aware CCA.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.0,
        event="refe",
        onset_event=True,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
    "literature_oriented_125": BenchmarkProfile(
        name="literature_oriented_125",
        description="Literature-inspired zero-training CCA profile at 125 Hz.",
        target_fs=125,
        band_low=6.0,
        band_high=50.0,
        notch_hz=50.0,
        drop_first_seconds=0.5,
        event="refe",
        onset_event=True,
        encoding_length=0.3,
        default_window_seconds_grid=THIELEN2021_KEY_WINDOWS,
    ),
}


def benchmark_profile_names() -> list[str]:
    return list(BENCHMARK_PROFILES)


def resolve_benchmark_profile(name: str) -> BenchmarkProfile:
    try:
        return BENCHMARK_PROFILES[name]
    except KeyError as exc:
        raise ValueError(f"Unknown benchmark profile {name}") from exc


def default_preprocessing_options() -> PreprocessingOptions:
    profile = resolve_benchmark_profile("legacy")
    return PreprocessingOptions(
        band_low=profile.band_low,
        band_high=profile.band_high,
        notch_hz=profile.notch_hz,
        pretrial_buffer_seconds=PRETRIAL_BUFFER_SECONDS,
        drop_first_seconds=profile.drop_first_seconds,
    )


def resolve_preprocessing_options(
    profile: BenchmarkProfile,
    *,
    band_low: float | None,
    band_high: float | None,
    notch_hz: float | None,
    drop_first_seconds: float | None,
) -> PreprocessingOptions:
    return PreprocessingOptions(
        band_low=profile.band_low if band_low is None else band_low,
        band_high=profile.band_high if band_high is None else band_high,
        notch_hz=profile.notch_hz if notch_hz is None else notch_hz,
        pretrial_buffer_seconds=PRETRIAL_BUFFER_SECONDS,
        drop_first_seconds=(
            profile.drop_first_seconds
            if drop_first_seconds is None
            else drop_first_seconds
        ),
    )


def resolve_window_grid(
    profile: BenchmarkProfile,
    dataset: str,
    explicit: list[float] | None,
    step_seconds: float | None,
) -> list[float] | None:
    if explicit is not None or step_seconds is not None:
        return explicit
    if dataset != "Thielen2021" or profile.default_window_seconds_grid is None:
        return explicit
    return list(profile.default_window_seconds_grid)


def resolve_target_fs(profile: BenchmarkProfile, target_fs: int | None) -> int:
    return profile.target_fs if target_fs is None else target_fs


def resolve_event(profile: BenchmarkProfile, event: str | None) -> str:
    return profile.event if event is None else event


def resolve_onset_event(profile: BenchmarkProfile, onset_event: bool | None) -> bool:
    return profile.onset_event if onset_event is None else onset_event


def resolve_encoding_length(
    profile: BenchmarkProfile, encoding_length: float | None
) -> float:
    return profile.encoding_length if encoding_length is None else encoding_length
