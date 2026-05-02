use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenReport {
    pub before: usize,
    pub after: usize,
    pub reduction_ratio: f64,
}

impl TokenReport {
    pub fn new(before: usize, after: usize) -> Self {
        let reduction_ratio = if before == 0 {
            0.0
        } else {
            1.0 - (after as f64 / before as f64)
        };
        Self {
            before,
            after,
            reduction_ratio,
        }
    }
}
