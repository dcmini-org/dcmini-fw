use cvep_decoder::{ChannelPreprocessor, SosCascade};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Fixture {
    channels: usize,
    sections: usize,
    sos_rows: Vec<[f32; 6]>,
    samples: Vec<Vec<f32>>,
}

#[derive(Serialize)]
struct ResultPayload {
    channels: usize,
    sections: usize,
    samples: usize,
    filtered: Vec<Vec<f32>>,
}

fn main() {
    let path = parse_fixture_path();
    let text = fs::read_to_string(&path).expect("failed to read preprocessing fixture");
    let fixture: Fixture =
        serde_json::from_str(&text).expect("failed to parse preprocessing fixture");
    let result = dispatch_fixture(&fixture);
    println!(
        "{}",
        serde_json::to_string_pretty(&result)
            .expect("failed to serialize preprocessing result")
    );
}

fn parse_fixture_path() -> PathBuf {
    let mut args = env::args_os();
    let _exe = args.next();
    let Some(path) = args.next() else {
        panic!("usage: preprocessing_fixture <fixture.json>");
    };
    PathBuf::from(path)
}

macro_rules! try_dispatch_fixture {
    ($fixture:expr, $channels:literal, [$($sections:literal),* $(,)?]) => {
        $(
            if $fixture.channels == $channels && $fixture.sections == $sections {
                return run_fixture::<$channels, $sections>($fixture);
            }
        )*
    };
}

fn dispatch_fixture(fixture: &Fixture) -> ResultPayload {
    try_dispatch_fixture!(fixture, 8, [1, 2, 3, 4, 5, 6, 7, 8]);
    try_dispatch_fixture!(fixture, 32, [1, 2, 3, 4, 5, 6, 7, 8]);
    try_dispatch_fixture!(fixture, 64, [1, 2, 3, 4, 5, 6, 7, 8]);
    panic!(
        "unsupported preprocessing shape channels={} sections={}",
        fixture.channels, fixture.sections
    );
}

fn run_fixture<const CHANNELS: usize, const SECTIONS: usize>(
    fixture: &Fixture,
) -> ResultPayload {
    assert_eq!(fixture.channels, CHANNELS);
    assert_eq!(fixture.sections, SECTIONS);
    assert_eq!(fixture.sos_rows.len(), SECTIONS);
    for frame in &fixture.samples {
        assert_eq!(frame.len(), CHANNELS);
    }

    let rows = fixture_rows_to_array::<SECTIONS>(&fixture.sos_rows);
    let cascade = SosCascade::<SECTIONS>::from_scipy_rows(rows);
    let mut preprocessor = ChannelPreprocessor::<CHANNELS, SECTIONS>::shared(cascade);

    let mut filtered = Vec::with_capacity(fixture.samples.len());
    for frame in &fixture.samples {
        let out = preprocessor.process_frame(frame_to_array::<CHANNELS>(frame));
        filtered.push(out.to_vec());
    }

    ResultPayload {
        channels: CHANNELS,
        sections: SECTIONS,
        samples: fixture.samples.len(),
        filtered,
    }
}

fn fixture_rows_to_array<const SECTIONS: usize>(rows: &[[f32; 6]]) -> [[f32; 6]; SECTIONS] {
    let mut out = [[0.0; 6]; SECTIONS];
    for (idx, row) in rows.iter().enumerate() {
        assert!(idx < SECTIONS);
        out[idx] = *row;
    }
    out
}

fn frame_to_array<const CHANNELS: usize>(frame: &[f32]) -> [f32; CHANNELS] {
    let mut out = [0.0; CHANNELS];
    for (idx, value) in frame.iter().enumerate() {
        assert!(idx < CHANNELS);
        out[idx] = *value;
    }
    out
}
