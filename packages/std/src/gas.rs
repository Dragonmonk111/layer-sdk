use thiserror::Error;

/// Tracks gas usage and returns error when it hits the limit
pub struct GasMeter {
    limit: u64,
    used: u64,
}

#[derive(Error, Debug)]
pub enum GasError {
    #[error("Out of gas")]
    OutOfGas,
}

impl GasMeter {
    pub fn new(limit: u64) -> Self {
        GasMeter { limit, used: 0 }
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    pub fn charge(&mut self, cost: u64) -> Result<(), GasError> {
        self.used += cost;
        if self.used >= self.limit {
            Err(GasError::OutOfGas)
        } else {
            Ok(())
        }
    }
}
