use crate::{
    errors::{Error, MayoError},
    parameterization::{ParameterSet, private},
    public_key::PublicKey,
    signature::Signature,
};
use alloc::{boxed::Box, collections::BTreeMap, vec, vec::Vec};
use bytes::Bytes;
use core::{fmt, marker::PhantomData};

/// Verifies many signatures while reusing expanded public keys.
///
/// MAYO verification first expands the compact public key into a much larger
/// internal representation. `BatchVerifier` caches that expanded key the first
/// time a public key is added. Later calls to [`add`](Self::add) with the same
/// public key only queue the message and signature.
///
/// This is an exact verifier, not a probabilistic batch check. Calling
/// [`verify`](Self::verify) verifies every queued `(public key, message,
/// signature)` tuple and returns `false` as soon as one tuple fails.
///
/// `BatchVerifier` is most useful when many signatures share one public key.
/// If every signature has a different public key, it still verifies correctly,
/// but there is less key-expansion work to amortize.
///
/// # Examples
///
/// ```rust
/// use rand_chacha::ChaCha20Rng;
/// use rand_core::SeedableRng;
/// use sriracha_mayo::{BatchVerifier, Mayo2, SecretKey};
///
/// let mut rng = ChaCha20Rng::from_seed([7; 32]);
/// let (public_key, secret_key) = SecretKey::<Mayo2>::random(&mut rng)?;
///
/// let first_message = b"first message";
/// let second_message = b"second message";
/// let first_signature = secret_key.sign(first_message)?;
/// let second_signature = secret_key.sign(second_message)?;
///
/// let mut batch = BatchVerifier::new();
/// batch.add(&public_key, first_message, &first_signature)?;
/// batch.add(&public_key, second_message, &second_signature)?;
///
/// assert!(batch.verify());
/// assert_eq!(batch.len(), 1);
/// # Ok::<(), sriracha_mayo::Error>(())
/// ```
pub struct BatchVerifier<P: ParameterSet> {
    groups: BTreeMap<Bytes, PublicKeyGroup<P>>,
}

impl<P: ParameterSet> fmt::Debug for BatchVerifier<P> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BatchVerifier")
            .field("groups", &self.groups.len())
            .finish()
    }
}

impl<P: ParameterSet> BatchVerifier<P> {
    /// Creates an empty batch verifier.
    ///
    /// No public keys are expanded until [`add`](Self::add) sees the first
    /// signature for a key.
    pub fn new() -> Self {
        Self {
            groups: BTreeMap::new(),
        }
    }

    /// Queues a signature verification.
    ///
    /// If `public_key` was already added to the batch, this only stores the
    /// message and signature. Otherwise, the public key is expanded once and
    /// cached for the rest of the batch.
    ///
    /// The message bytes are copied into the batch. The public key and
    /// signature are cheap clones of immutable byte buffers.
    ///
    /// # Errors
    ///
    /// Returns [`Error::PublicKeyExpansion`] if MAYO-C rejects a newly added
    /// public key while expanding it.
    pub fn add(
        &mut self,
        public_key: &PublicKey<P>,
        message: impl AsRef<[u8]>,
        signature: &Signature<P>,
    ) -> Result<(), Error> {
        let message = Bytes::copy_from_slice(message.as_ref());

        if let Some(group) = self.groups.get_mut(&public_key.bytes) {
            group.items.push(QueuedVerification {
                message,
                signature: signature.clone(),
            });
            return Ok(());
        }

        self.groups.insert(
            public_key.bytes.clone(),
            PublicKeyGroup {
                expanded_public_key: ExpandedPublicKey::new(public_key)?,
                items: vec![QueuedVerification {
                    message,
                    signature: signature.clone(),
                }],
            },
        );

        Ok(())
    }

    /// Verifies every queued signature.
    ///
    /// Returns `true` when the batch is empty.
    pub fn verify(&self) -> bool {
        for group in self.groups.values() {
            if !group.verify() {
                return false;
            }
        }

        true
    }

    /// Returns the number of distinct public keys in the batch.
    ///
    /// This is not the number of queued signatures. Multiple signatures under
    /// the same public key count as one group.
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// Returns true when no public keys are queued.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

impl<P: ParameterSet> Default for BatchVerifier<P> {
    fn default() -> Self {
        Self::new()
    }
}

struct PublicKeyGroup<P: ParameterSet> {
    expanded_public_key: ExpandedPublicKey<P>,
    items: Vec<QueuedVerification<P>>,
}

impl<P: ParameterSet> PublicKeyGroup<P> {
    fn verify(&self) -> bool {
        for item in &self.items {
            if !self
                .expanded_public_key
                .verify(&item.signature, &item.message)
            {
                return false;
            }
        }

        true
    }
}

struct QueuedVerification<P: ParameterSet> {
    message: Bytes,
    signature: Signature<P>,
}

struct ExpandedPublicKey<P: ParameterSet> {
    words: Box<[u64]>,
    parameter_set: PhantomData<P>,
}

impl<P: ParameterSet> ExpandedPublicKey<P> {
    fn new(public_key: &PublicKey<P>) -> Result<Self, Error> {
        let mut words = vec![0; P::EXPANDED_PUBLIC_KEY_WORDS].into_boxed_slice();

        // SAFETY: The expanded-key buffer is allocated with `u64` alignment and
        // the exact number of words required by `P`. The compact public key was
        // constructed with `P`'s public-key size. MAYO-C does not retain either
        // pointer.
        let result = unsafe {
            <P as private::Sealed>::EXPAND_PUBLIC_KEY(words.as_mut_ptr(), public_key.bytes.as_ptr())
        };
        if result != 0 {
            return Err(Error::PublicKeyExpansion {
                error: MayoError::from_code(result),
            });
        }

        Ok(Self {
            words,
            parameter_set: PhantomData,
        })
    }

    fn verify(&self, signature: &Signature<P>, message: &[u8]) -> bool {
        // SAFETY: The signature has the exact length required by `P`, the
        // message pointer is paired with its length, and `self.words` contains a
        // successful MAYO-C expanded public key. MAYO-C does not retain any
        // pointer after the call.
        let result = unsafe {
            <P as private::Sealed>::VERIFY_EXPANDED(
                signature.bytes.as_ptr(),
                signature.bytes.len(),
                message.as_ptr(),
                message.len(),
                self.words.as_ptr(),
            )
        };

        result == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Mayo5;
    use core::mem::size_of;

    #[test]
    fn expanded_public_key_handle_stays_small() {
        assert!(size_of::<ExpandedPublicKey<Mayo5>>() <= 32);
    }
}
