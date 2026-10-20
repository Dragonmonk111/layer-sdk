use crate::{
    errors::{Error, MayoError, expect_len},
    parameterization::{ParameterSet, private},
    public_key::PublicKey,
    signature::Signature,
};
use bytes::BytesMut;
use core::{marker::PhantomData, mem::MaybeUninit};
use rand_core::CryptoRngCore;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A compact MAYO secret key.
///
/// The key material is zeroized when the value is dropped.
///
/// Secret keys use MAYO's compact secret-seed encoding. Use
/// [`AsRef<[u8]>`](AsRef) to read the encoded key bytes, and
/// [`TryFrom<&[u8]>`](TryFrom) to parse a key from bytes. Parsing checks that
/// the input length matches the selected parameter set.
///
/// `SecretKey` intentionally does not implement [`Debug`](core::fmt::Debug),
/// because debug output should not expose secret key material.
///
/// When the `serde` feature is enabled, secret keys serialize as their compact
/// byte encoding. Deserialized keys are zeroized when dropped.
#[derive(Zeroize, ZeroizeOnDrop)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct SecretKey<P: ParameterSet> {
    bytes: BytesMut,
    #[cfg_attr(feature = "serde", serde(skip))]
    #[zeroize(skip)]
    parameter_set: PhantomData<P>,
}

impl<P: ParameterSet> Clone for SecretKey<P> {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            parameter_set: PhantomData,
        }
    }
}

impl<P: ParameterSet> SecretKey<P> {
    /// Derives a keypair from a compact secret-key seed.
    ///
    /// MAYO compact secret keys are seeds. MAYO-C derives the matching compact
    /// public key deterministically from that seed, so this constructor returns
    /// both halves and prevents callers from pairing mismatched key bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidLength`] if `seed` does not have the exact
    /// secret-key length for `P`. Returns [`Error::KeyGeneration`] if MAYO-C
    /// fails to derive the matching public key.
    pub fn from_seed(seed: &[u8]) -> Result<(PublicKey<P>, Self), Error> {
        expect_len(P::SECRET_KEY_BYTES, seed.len())?;

        let mut public_key = BytesMut::zeroed(P::PUBLIC_KEY_BYTES);
        let mut secret_key = BytesMut::from(seed);

        // SAFETY: The secret-key buffer has the exact seed length required by
        // `P`, and the public-key output buffer is allocated to `P`'s compact
        // public-key size. MAYO-C does not retain either pointer.
        let result = unsafe {
            <P as private::Sealed>::SEEDED_KEYPAIR(public_key.as_mut_ptr(), secret_key.as_ptr())
        };
        if result != 0 {
            secret_key.zeroize();
            return Err(Error::KeyGeneration {
                error: MayoError::from_code(result),
            });
        }

        Ok((
            PublicKey {
                bytes: public_key.freeze(),
                parameter_set: PhantomData,
            },
            Self {
                bytes: secret_key,
                parameter_set: PhantomData,
            },
        ))
    }

    /// Generates a keypair using caller-provided randomness.
    ///
    /// The RNG fills MAYO's compact secret-key seed. MAYO-C derives the
    /// matching compact public key deterministically from that seed.
    ///
    /// This is the only key-generation API. The crate does not call MAYO-C's
    /// process-global randomness path.
    ///
    /// # Errors
    ///
    /// Returns [`Error::KeyGeneration`] if MAYO-C fails to derive the public key
    /// from the generated secret seed.
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
    ///
    /// let message = b"message";
    /// let signature = secret_key.sign(message)?;
    /// assert!(signature.verify(&public_key, message));
    /// # Ok::<(), sriracha_mayo::Error>(())
    /// ```
    pub fn random<R>(rng: &mut R) -> Result<(PublicKey<P>, Self), Error>
    where
        R: CryptoRngCore,
    {
        let mut secret_key = BytesMut::zeroed(P::SECRET_KEY_BYTES);

        rng.fill_bytes(&mut secret_key);

        Self::from_seed(&secret_key)
    }

    /// Signs `message` and returns a detached signature.
    ///
    /// Key-generation randomness comes from [`random`](Self::random). MAYO-C
    /// still performs the per-signature salt generation used by the MAYO
    /// signing algorithm.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Signing`] if MAYO-C fails to produce a signature, or
    /// [`Error::InvalidLength`] if MAYO-C reports an unexpected signature
    /// length.
    pub fn sign(&self, message: &[u8]) -> Result<Signature<P>, Error> {
        let mut signature = BytesMut::zeroed(P::SIGNATURE_BYTES);
        let mut signature_len = MaybeUninit::<usize>::uninit();

        // SAFETY: The signature output buffer is allocated to `P`'s signature
        // size, `signature_len` points to valid uninitialized storage, and the
        // message and secret-key pointers are read-only with correct lengths.
        // MAYO-C initializes `signature_len` before returning success.
        let result = unsafe {
            <P as private::Sealed>::SIGN(
                signature.as_mut_ptr(),
                signature_len.as_mut_ptr(),
                message.as_ptr(),
                message.len(),
                self.bytes.as_ptr(),
            )
        };
        if result != 0 {
            return Err(Error::Signing {
                error: MayoError::from_code(result),
            });
        }

        // SAFETY: MAYO-C returned success, which means it initialized the
        // `signature_len` out-parameter.
        let signature_len = unsafe { signature_len.assume_init() };
        if signature_len != P::SIGNATURE_BYTES {
            return Err(Error::InvalidLength {
                expected: P::SIGNATURE_BYTES,
                actual: signature_len,
            });
        }

        Ok(Signature {
            bytes: signature.freeze(),
            parameter_set: PhantomData,
        })
    }
}

impl<P: ParameterSet> AsRef<[u8]> for SecretKey<P> {
    /// Returns the compact secret-key bytes.
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl<P: ParameterSet> TryFrom<&[u8]> for SecretKey<P> {
    type Error = Error;

    /// Parses a compact secret key from bytes.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidLength`] if `bytes` does not have the exact
    /// secret-key length for `P`.
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        expect_len(P::SECRET_KEY_BYTES, bytes.len())?;

        Ok(Self {
            bytes: BytesMut::from(bytes),
            parameter_set: PhantomData,
        })
    }
}
