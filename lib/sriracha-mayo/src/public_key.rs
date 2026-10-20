use crate::{
    errors::{Error, expect_len},
    parameterization::ParameterSet,
};
use bytes::Bytes;
use core::marker::PhantomData;

/// A compact MAYO public key.
///
/// Public keys use MAYO's compact byte encoding. Verification expands this
/// compact encoding internally before evaluating the public map.
///
/// Use [`AsRef<[u8]>`](AsRef) to read the encoded key bytes, and
/// [`TryFrom<&[u8]>`](TryFrom) to parse a key from bytes. Parsing checks that
/// the input length matches the selected parameter set.
///
/// When the `serde` feature is enabled, public keys serialize as their compact
/// byte encoding.
///
/// # Examples
///
/// ```rust
/// use sriracha_mayo::{Mayo2, PublicKey, SecretKey};
/// use rand_chacha::ChaCha20Rng;
/// use rand_core::SeedableRng;
///
/// let mut rng = ChaCha20Rng::from_seed([7; 32]);
/// let (public_key, _) = SecretKey::<Mayo2>::random(&mut rng)?;
/// let encoded = public_key.as_ref();
///
/// let parsed = PublicKey::<Mayo2>::try_from(encoded)?;
/// assert_eq!(parsed.as_ref(), encoded);
/// # Ok::<(), sriracha_mayo::Error>(())
/// ```
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct PublicKey<P: ParameterSet> {
    pub(crate) bytes: Bytes,
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) parameter_set: PhantomData<P>,
}

impl<P: ParameterSet> Clone for PublicKey<P> {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            parameter_set: PhantomData,
        }
    }
}

impl<P: ParameterSet> AsRef<[u8]> for PublicKey<P> {
    /// Returns the compact public-key bytes.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl<P: ParameterSet> TryFrom<&[u8]> for PublicKey<P> {
    type Error = Error;

    /// Parses a compact public key from bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidLength`] if `bytes` does not have the exact
    /// public-key length for `P`.
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        expect_len(P::PUBLIC_KEY_BYTES, bytes.len())?;

        Ok(Self {
            bytes: Bytes::copy_from_slice(bytes),
            parameter_set: PhantomData,
        })
    }
}
