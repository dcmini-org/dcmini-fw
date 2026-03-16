use crate::types::Decision;

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum CumulativeCcaUpdatePolicy {
    AlwaysUpdate,
    MarginThreshold(f32),
}

impl Default for CumulativeCcaUpdatePolicy {
    fn default() -> Self {
        Self::AlwaysUpdate
    }
}

impl CumulativeCcaUpdatePolicy {
    pub fn should_update(&self, decision: &Decision) -> bool {
        match self {
            Self::AlwaysUpdate => true,
            Self::MarginThreshold(threshold) => decision.margin >= *threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CumulativeCcaUpdatePolicy;
    use crate::types::Decision;

    #[test]
    fn margin_threshold_requires_sufficient_margin() {
        let policy = CumulativeCcaUpdatePolicy::MarginThreshold(0.25);
        let decision = Decision {
            class_index: 0,
            raw_score: 1,
            normalized_score: 0.8,
            margin: 0.2,
        };
        assert!(!policy.should_update(&decision));
        assert!(
            CumulativeCcaUpdatePolicy::AlwaysUpdate.should_update(&decision)
        );
    }
}
