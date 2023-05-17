use thiserror::Error;

/// Tracks gas usage and returns error when it hits the limit
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GasMeter {
    limit: u64,
    used: u64,
}

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum GasError {
    #[error("Out of gas")]
    OutOfGas,
}

pub type GasResult<T> = Result<T, GasError>;

impl GasMeter {
    pub fn new(limit: u64) -> Self {
        GasMeter { limit, used: 0 }
    }

    pub fn infinite() -> Self {
        GasMeter::new(u64::MAX)
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
