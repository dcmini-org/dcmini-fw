use libm::sqrtf;

/// Exact single-component projected-correlation parameters organized as
/// `[class][channel]` and `[class][time]`.
pub struct ProjectedCorrelationBank<
    const CLASSES: usize,
    const CHANNELS: usize,
    const WINDOW: usize,
> {
    pub(crate) spatial_filters: [[f32; CHANNELS]; CLASSES],
    pub(crate) templates: [[f32; WINDOW]; CLASSES],
    pub(crate) template_norms: [f32; CLASSES],
}

impl<const CLASSES: usize, const CHANNELS: usize, const WINDOW: usize>
    ProjectedCorrelationBank<CLASSES, CHANNELS, WINDOW>
{
    pub fn new(
        spatial_filters: [[f32; CHANNELS]; CLASSES],
        mut templates: [[f32; WINDOW]; CLASSES],
    ) -> Self {
        let mut template_norms = [1.0; CLASSES];
        let mut class_idx = 0;
        while class_idx < CLASSES {
            let mut mean = 0.0f32;
            let mut sample_idx = 0;
            while sample_idx < WINDOW {
                mean += templates[class_idx][sample_idx];
                sample_idx += 1;
            }
            mean /= WINDOW as f32;

            let mut energy = 0.0f32;
            let mut sample_idx = 0;
            while sample_idx < WINDOW {
                templates[class_idx][sample_idx] -= mean;
                let value = templates[class_idx][sample_idx];
                energy += value * value;
                sample_idx += 1;
            }
            if energy > 0.0 {
                template_norms[class_idx] = sqrtf(energy);
            }
            class_idx += 1;
        }

        Self { spatial_filters, templates, template_norms }
    }

    pub fn spatial_filters(&self) -> &[[f32; CHANNELS]; CLASSES] {
        &self.spatial_filters
    }

    pub fn templates(&self) -> &[[f32; WINDOW]; CLASSES] {
        &self.templates
    }
}

pub type EtRcaBank<const CLASSES: usize, const CHANNELS: usize, const WINDOW: usize> =
    ProjectedCorrelationBank<CLASSES, CHANNELS, WINDOW>;
pub type RccaBank<const CLASSES: usize, const CHANNELS: usize, const WINDOW: usize> =
    ProjectedCorrelationBank<CLASSES, CHANNELS, WINDOW>;

/// Binary target / non-target labels over stimulus-locked epochs.
///
/// `labels[class][epoch] != 0` marks epochs that should behave like target
/// responses under the given class hypothesis.
#[derive(Clone, Copy)]
pub struct UmmCodebook<'a, const CLASSES: usize, const EPOCHS: usize> {
    pub(crate) labels: &'a [[u8; EPOCHS]; CLASSES],
}

impl<'a, const CLASSES: usize, const EPOCHS: usize>
    UmmCodebook<'a, CLASSES, EPOCHS>
{
    pub const fn new(labels: &'a [[u8; EPOCHS]; CLASSES]) -> Self {
        Self { labels }
    }

    pub fn labels(&self) -> &'a [[u8; EPOCHS]; CLASSES] {
        self.labels
    }
}

/// Precomputed per-class urCCA encoding matrices for a fixed trial length.
///
/// `encodings[class][feature][time]` should be exported from the host using the
/// same event and encoding settings as the reference PyntBCI pipeline.
#[derive(Clone, Copy)]
pub struct UrCcaBank<
    'a,
    const CLASSES: usize,
    const FEATURES: usize,
    const WINDOW: usize,
> {
    pub(crate) encodings: &'a [[[f32; WINDOW]; FEATURES]; CLASSES],
}

impl<'a, const CLASSES: usize, const FEATURES: usize, const WINDOW: usize>
    UrCcaBank<'a, CLASSES, FEATURES, WINDOW>
{
    pub const fn new(
        encodings: &'a [[[f32; WINDOW]; FEATURES]; CLASSES],
    ) -> Self {
        Self { encodings }
    }

    pub fn encodings(&self) -> &'a [[[f32; WINDOW]; FEATURES]; CLASSES] {
        self.encodings
    }
}
