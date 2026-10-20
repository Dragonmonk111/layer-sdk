use thiserror::Error;

/// A typed MAYO-C error code.
///
/// MAYO-C currently exposes one documented nonzero error status. The wrapper
/// keeps that status typed so callers can distinguish it from unexpected
/// status codes returned by a future or modified C backend.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MayoError {
    /// The documented `MAYO_ERR` status from MAYO-C.
    ///
    /// MAYO-C uses this opaque status for key generation, signing, and public
    /// key expansion failures. The surrounding [`enum@Error`] variant identifies
    /// which operation failed.
    #[error("MAYO_ERR")]
    MayoErr,

    /// An unrecognized nonzero MAYO-C status.
    ///
    /// This preserves the raw code instead of collapsing it into
    /// [`MayoError::MayoErr`], which keeps the binding forward-compatible with
    /// backends that add more specific error values.
    #[error("unknown MAYO-C error code {0}")]
    Unknown(i32),
}

impl MayoError {
    pub(crate) fn from_code(code: i32) -> Self {
        if code == MAYO_ERR {
            return Self::MayoErr;
        }

        Self::Unknown(code)
    }

    /// Returns the MAYO-C integer status code.
    pub fn code(self) -> i32 {
        match self {
            Self::MayoErr => MAYO_ERR,
            Self::Unknown(code) => code,
        }
    }
}

/// A MAYO operation failed.
///
/// Verification failure is not represented as an error because invalid
/// signatures are expected input. [`crate::Signature::verify`] returns `false`
/// for invalid signatures and only these fallible construction or signing
/// operations use `Result`.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum Error {
    /// MAYO-C returned an error while generating a keypair.
    #[error("MAYO-C failed to generate a keypair: {error}")]
    KeyGeneration {
        /// The MAYO-C error code.
        #[source]
        error: MayoError,
    },
    /// MAYO-C returned an error while signing a message.
    #[error("MAYO-C failed to sign the message: {error}")]
    Signing {
        /// The MAYO-C error code.
        #[source]
        error: MayoError,
    },
    /// MAYO-C returned an error while expanding a public key.
    #[error("MAYO-C failed to expand the public key: {error}")]
    PublicKeyExpansion {
        /// The MAYO-C error code.
        #[source]
        error: MayoError,
    },
    /// A byte slice had the wrong length for the selected parameter set.
    ///
    /// This is returned by [`TryFrom`] implementations for
    /// [`crate::PublicKey`], [`crate::SecretKey`], and [`crate::Signature`].
    #[error("invalid byte length: expected {expected}, got {actual}")]
    InvalidLength {
        /// The expected number of bytes.
        expected: usize,
        /// The actual number of bytes.
        actual: usize,
    },
}

pub(crate) fn expect_len(expected: usize, actual: usize) -> Result<(), Error> {
    if actual == expected {
        return Ok(());
    }

    Err(Error::InvalidLength { expected, actual })
}

const MAYO_ERR: i32 = 1;
