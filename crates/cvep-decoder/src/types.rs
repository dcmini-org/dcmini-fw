use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decision {
    pub class_index: usize,
    pub raw_score: i64,
    pub normalized_score: f32,
    pub margin: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    NotReady,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotReady => f.write_str("decoder window not filled"),
        }
    }
}
