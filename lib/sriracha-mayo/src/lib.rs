#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://avatars.githubusercontent.com/u/115088230",
    html_favicon_url = "https://pqmayo.org/assets/favicon.ico"
)]
#![cfg_attr(not(any(feature = "std", test)), no_std)]

extern crate alloc;

mod errors;
pub use errors::{Error, MayoError};

mod batch_verify;
pub use batch_verify::BatchVerifier;

mod parameterization;
pub use parameterization::{Mayo1, Mayo2, Mayo3, Mayo5, ParameterSet};

mod public_key;
pub use public_key::PublicKey;

mod secret_key;
pub use secret_key::SecretKey;

mod signature;
pub use signature::Signature;

mod externs;

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Debug;
    use rand_chacha::ChaCha20Rng;
    use rand_core::SeedableRng;
    use rstest::rstest;

    #[rstest]
    #[case::mayo_1(roundtrip::<Mayo1>)]
    #[case::mayo_2(roundtrip::<Mayo2>)]
    #[case::mayo_3(roundtrip::<Mayo3>)]
    #[case::mayo_5(roundtrip::<Mayo5>)]
    fn signs_and_verifies(#[case] test: fn()) {
        run_on_large_stack(test);
    }

    fn roundtrip<P: ParameterSet>() {
        let mut rng = ChaCha20Rng::from_seed([1; 32]);
        let message = b"mayo signing test";
        let (public_key, secret_key) = SecretKey::<P>::random(&mut rng).unwrap();
        let signature = secret_key.sign(message).unwrap();

        assert!(signature.verify(&public_key, message));
        assert!(!signature.verify(&public_key, b"wrong message"));
    }

    #[rstest]
    #[case::mayo_1(seeded_roundtrip::<Mayo1>)]
    #[case::mayo_2(seeded_roundtrip::<Mayo2>)]
    #[case::mayo_3(seeded_roundtrip::<Mayo3>)]
    #[case::mayo_5(seeded_roundtrip::<Mayo5>)]
    fn seeded_keygen_signs_and_verifies(#[case] test: fn()) {
        run_on_large_stack(test);
    }

    fn seeded_roundtrip<P: ParameterSet>() {
        let mut rng = ChaCha20Rng::from_seed([9; 32]);
        let (public_key, secret_key) = SecretKey::<P>::random(&mut rng).unwrap();
        let message = b"seeded keygen roundtrip";
        let signature = secret_key.sign(message).unwrap();

        assert!(signature.verify(&public_key, message));
    }

    #[rstest]
    #[case::mayo_1(batch_roundtrip::<Mayo1>)]
    #[case::mayo_2(batch_roundtrip::<Mayo2>)]
    #[case::mayo_3(batch_roundtrip::<Mayo3>)]
    #[case::mayo_5(batch_roundtrip::<Mayo5>)]
    fn batch_verifies_duplicate_public_keys(#[case] test: fn()) {
        run_on_large_stack(test);
    }

    fn batch_roundtrip<P: ParameterSet>() {
        let mut rng = ChaCha20Rng::from_seed([2; 32]);
        let (public_key, secret_key) = SecretKey::<P>::random(&mut rng).unwrap();
        let first_message = b"first batch message";
        let second_message = b"second batch message";
        let first_signature = secret_key.sign(first_message).unwrap();
        let second_signature = secret_key.sign(second_message).unwrap();

        let mut batch = BatchVerifier::new();
        batch
            .add(&public_key, first_message, &first_signature)
            .unwrap();
        batch
            .add(&public_key, second_message, &second_signature)
            .unwrap();

        assert_eq!(batch.len(), 1);
        assert!(batch.verify());

        let mut invalid_batch = BatchVerifier::new();
        invalid_batch
            .add(&public_key, b"wrong message", &first_signature)
            .unwrap();

        assert!(!invalid_batch.verify());
    }

    #[test]
    fn rejects_wrong_lengths() {
        assert!(matches!(
            PublicKey::<Mayo1>::try_from(&[][..]),
            Err(Error::InvalidLength {
                expected: Mayo1::PUBLIC_KEY_BYTES,
                actual: 0,
            })
        ));
        assert!(matches!(
            SecretKey::<Mayo1>::try_from(&[][..]),
            Err(Error::InvalidLength {
                expected: Mayo1::SECRET_KEY_BYTES,
                actual: 0,
            })
        ));
        assert!(matches!(
            Signature::<Mayo1>::try_from(&[][..]),
            Err(Error::InvalidLength {
                expected: Mayo1::SIGNATURE_BYTES,
                actual: 0,
            })
        ));
    }

    #[test]
    fn empty_messages_can_be_signed() {
        run_on_large_stack(|| {
            let mut rng = ChaCha20Rng::from_seed([3; 32]);
            let (public_key, secret_key) = SecretKey::<Mayo1>::random(&mut rng).unwrap();
            let signature = secret_key.sign(&[]).unwrap();

            assert!(signature.verify(&public_key, &[]));
        });
    }

    #[test]
    fn random_key_generation_uses_caller_rng() {
        run_on_large_stack(|| {
            let mut first_rng = ChaCha20Rng::from_seed([7; 32]);
            let mut second_rng = ChaCha20Rng::from_seed([7; 32]);
            let mut different_rng = ChaCha20Rng::from_seed([8; 32]);

            let (first_public_key, first_secret_key) =
                SecretKey::<Mayo3>::random(&mut first_rng).unwrap();
            let (second_public_key, second_secret_key) =
                SecretKey::<Mayo3>::random(&mut second_rng).unwrap();
            let (different_public_key, different_secret_key) =
                SecretKey::<Mayo3>::random(&mut different_rng).unwrap();

            assert_eq!(first_public_key.as_ref(), second_public_key.as_ref());
            assert_eq!(first_secret_key.as_ref(), second_secret_key.as_ref());
            assert_ne!(first_public_key.as_ref(), different_public_key.as_ref());
            assert_ne!(first_secret_key.as_ref(), different_secret_key.as_ref());

            let message = b"seeded keygen test";
            let signature = first_secret_key.sign(message).unwrap();
            assert!(signature.verify(&first_public_key, message));
        });
    }

    #[test]
    fn keypairs_can_be_derived_from_secret_key_seeds() {
        run_on_large_stack(|| {
            let seed = [11; Mayo5::SECRET_KEY_BYTES];
            let (public_key, secret_key) = SecretKey::<Mayo5>::from_seed(&seed).unwrap();
            let (same_public_key, same_secret_key) = SecretKey::<Mayo5>::from_seed(&seed).unwrap();
            let message = b"seeded constructor";
            let signature = secret_key.sign(message).unwrap();

            assert_eq!(secret_key.as_ref(), seed);
            assert_eq!(same_secret_key.as_ref(), seed);
            assert_eq!(public_key, same_public_key);
            assert!(signature.verify(&public_key, message));
        });
    }

    #[test]
    fn seeded_keypair_derivation_rejects_wrong_lengths() {
        assert!(matches!(
            SecretKey::<Mayo1>::from_seed(&[]),
            Err(Error::InvalidLength {
                expected: Mayo1::SECRET_KEY_BYTES,
                actual: 0,
            })
        ));
    }

    #[test]
    fn binds_mayo_error_codes() {
        assert_eq!(MayoError::MayoErr.code(), 1);
        assert_eq!(MayoError::Unknown(7).code(), 7);
    }

    #[test]
    fn concrete_public_types_expose_derived_traits() {
        fn assert_public_traits<T: Debug + Eq + PartialEq>() {}
        fn assert_secret_key_traits<T: Clone>() {}
        fn assert_parameter_set<P: ParameterSet + Debug + Eq + PartialEq>() {
            assert_public_traits::<P>();
            assert_public_traits::<PublicKey<P>>();
            assert_public_traits::<Signature<P>>();
            assert_secret_key_traits::<SecretKey<P>>();
        }

        assert_parameter_set::<Mayo1>();
        assert_parameter_set::<Mayo2>();
        assert_parameter_set::<Mayo3>();
        assert_parameter_set::<Mayo5>();
    }

    #[test]
    fn cloned_secret_keys_keep_the_same_seed() {
        run_on_large_stack(|| {
            let mut rng = ChaCha20Rng::from_seed([12; 32]);
            let (public_key, secret_key) = SecretKey::<Mayo3>::random(&mut rng).unwrap();
            let cloned_secret_key = secret_key.clone();
            let message = b"cloned signer";
            let signature = cloned_secret_key.sign(message).unwrap();

            assert_eq!(secret_key.as_ref(), cloned_secret_key.as_ref());
            assert!(signature.verify(&public_key, message));
        });
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_impls_cover_all_parameter_sets() {
        use serde::{Serialize, de::DeserializeOwned};

        fn assert_serde<T: Serialize + DeserializeOwned>() {}
        fn assert_parameter_set<P: ParameterSet>() {
            assert_serde::<PublicKey<P>>();
            assert_serde::<SecretKey<P>>();
            assert_serde::<Signature<P>>();
        }

        assert_parameter_set::<Mayo1>();
        assert_parameter_set::<Mayo2>();
        assert_parameter_set::<Mayo3>();
        assert_parameter_set::<Mayo5>();
    }

    fn run_on_large_stack(test: impl FnOnce() + Send + 'static) {
        std::thread::Builder::new()
            .stack_size(32 * 1024 * 1024)
            .spawn(test)
            .unwrap()
            .join()
            .unwrap();
    }
}
