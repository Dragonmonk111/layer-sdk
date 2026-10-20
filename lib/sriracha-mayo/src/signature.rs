use crate::{
    errors::{Error, expect_len},
    parameterization::{ParameterSet, private},
    public_key::PublicKey,
};
use bytes::Bytes;
use core::marker::PhantomData;

/// A detached MAYO signature.
///
/// Signatures use MAYO's detached byte encoding. Use
/// [`AsRef<[u8]>`](AsRef) to read the encoded signature bytes, and
/// [`TryFrom<&[u8]>`](TryFrom) to parse a signature from bytes. Parsing checks
/// that the input length matches the selected parameter set.
///
/// When the `serde` feature is enabled, signatures serialize as their detached
/// byte encoding.
///
/// # Examples
///
/// ```rust
/// use sriracha_mayo::{Mayo2, SecretKey, Signature};
/// use rand_chacha::ChaCha20Rng;
/// use rand_core::SeedableRng;
///
/// let mut rng = ChaCha20Rng::from_seed([7; 32]);
/// let (_, secret_key) = SecretKey::<Mayo2>::random(&mut rng)?;
/// let signature = secret_key.sign(b"message")?;
/// let encoded = signature.as_ref();
///
/// let parsed = Signature::<Mayo2>::try_from(encoded)?;
/// assert_eq!(parsed.as_ref(), encoded);
/// # Ok::<(), sriracha_mayo::Error>(())
/// ```
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Signature<P: ParameterSet> {
    pub(crate) bytes: Bytes,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) parameter_set: PhantomData<P>,
}

impl<P: ParameterSet> Clone for Signature<P> {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            parameter_set: PhantomData,
        }
    }
}

impl<P: ParameterSet> Signature<P> {
    /// Verifies this signature for `message` with `public_key`.
    ///
    /// Returns `false` for any invalid signature, wrong message, or wrong
    /// public key. Verification failures are not reported as errors because
    /// they are expected data outcomes.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use sriracha_mayo::{Mayo1, SecretKey};
    /// use rand_chacha::ChaCha20Rng;
    /// use rand_core::SeedableRng;
    ///
    /// let mut rng = ChaCha20Rng::from_seed([7; 32]);
    /// let (public_key, secret_key) = SecretKey::<Mayo1>::random(&mut rng)?;
    /// let signature = secret_key.sign(b"message")?;
    ///
    /// assert!(signature.verify(&public_key, b"message"));
    /// assert!(!signature.verify(&public_key, b"other message"));
    /// # Ok::<(), sriracha_mayo::Error>(())
    /// ```
    pub fn verify(&self, public_key: &PublicKey<P>, message: &[u8]) -> bool {
        // SAFETY: The key and signature buffers are constructed with the exact
        // lengths required by `P`. Message bytes are read-only and paired with
        // their length. MAYO-C does not retain any pointer after the call.
        let result = unsafe {
            <P as private::Sealed>::VERIFY(
                self.bytes.as_ptr(),
                self.bytes.len(),
                message.as_ptr(),
                message.len(),
                public_key.bytes.as_ptr(),
            )
        };

        result == 0
    }
}

impl<P: ParameterSet> AsRef<[u8]> for Signature<P> {
    /// Returns the detached signature bytes.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl<P: ParameterSet> TryFrom<&[u8]> for Signature<P> {
    type Error = Error;

    /// Parses a detached signature from bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidLength`] if `bytes` does not have the exact
    /// signature length for `P`.
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        expect_len(P::SIGNATURE_BYTES, bytes.len())?;

        Ok(Self {
            bytes: Bytes::copy_from_slice(bytes),
            parameter_set: PhantomData,
        })
    }
}
