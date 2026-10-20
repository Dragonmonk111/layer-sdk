use crate::externs;

/// A marker trait for MAYO parameter sets.
///
/// Use [`Mayo1`], [`Mayo2`], [`Mayo3`], or [`Mayo5`] as the type
/// parameter for [`crate::PublicKey`], [`crate::SecretKey`], and
/// [`crate::Signature`].
///
/// This trait is sealed. Downstream crates cannot implement custom parameter
/// sets because the wrapper only exposes the four MAYO-C parameter sets that
/// are linked into the crate.
pub trait ParameterSet: private::Sealed {
    /// Human-readable algorithm name.
    const NAME: &'static str;

    /// Compact secret-key encoding size in bytes.
    const SECRET_KEY_BYTES: usize;

    /// Compact public-key encoding size in bytes.
    const PUBLIC_KEY_BYTES: usize;

    /// Detached signature size in bytes.
    const SIGNATURE_BYTES: usize;
}

/// MAYO-1 marker type.
///
/// MAYO-1 uses 24-byte secret keys, 1420-byte public keys, and 454-byte
/// detached signatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mayo1;

/// MAYO-2 marker type.
///
/// MAYO-2 uses 24-byte secret keys, 4912-byte public keys, and 186-byte
/// detached signatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mayo2;

/// MAYO-3 marker type.
///
/// MAYO-3 uses 32-byte secret keys, 2986-byte public keys, and 681-byte
/// detached signatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mayo3;

/// MAYO-5 marker type.
///
/// MAYO-5 uses 40-byte secret keys, 5554-byte public keys, and 964-byte
/// detached signatures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mayo5;

macro_rules! parameter_set {
    (
        $type:ty,
        $name:literal,
        $secret_key_bytes:literal,
        $public_key_bytes:literal,
        $signature_bytes:literal,
        $expanded_public_key_words:literal,
        $seeded_keypair:ident,
        $expand_public_key:ident,
        $sign:ident,
        $verify:ident,
        $verify_expanded:ident
    ) => {
        impl private::Sealed for $type {
            const SEEDED_KEYPAIR: unsafe extern "C" fn(*mut u8, *const u8) -> i32 =
                externs::$seeded_keypair;
            const EXPANDED_PUBLIC_KEY_WORDS: usize = $expanded_public_key_words;
            const EXPAND_PUBLIC_KEY: unsafe extern "C" fn(*mut u64, *const u8) -> i32 =
                externs::$expand_public_key;
            const SIGN: unsafe extern "C" fn(
                *mut u8,
                *mut usize,
                *const u8,
                usize,
                *const u8,
            ) -> i32 = externs::$sign;
            const VERIFY: unsafe extern "C" fn(
                *const u8,
                usize,
                *const u8,
                usize,
                *const u8,
            ) -> i32 = externs::$verify;
            const VERIFY_EXPANDED: unsafe extern "C" fn(
                *const u8,
                usize,
                *const u8,
                usize,
                *const u64,
            ) -> i32 = externs::$verify_expanded;
        }

        impl ParameterSet for $type {
            const NAME: &'static str = $name;
            const SECRET_KEY_BYTES: usize = $secret_key_bytes;
            const PUBLIC_KEY_BYTES: usize = $public_key_bytes;
            const SIGNATURE_BYTES: usize = $signature_bytes;
        }
    };
}

parameter_set!(
    Mayo1,
    "MAYO-1",
    24,
    1420,
    454,
    18705,
    mayo_1_seeded_keypair,
    mayo_1_expand_public_key,
    mayo_1_sign,
    mayo_1_verify,
    mayo_1_verify_expanded
);
parameter_set!(
    Mayo2,
    "MAYO-2",
    24,
    4912,
    186,
    13284,
    mayo_2_seeded_keypair,
    mayo_2_expand_public_key,
    mayo_2_sign,
    mayo_2_verify,
    mayo_2_verify_expanded
);
parameter_set!(
    Mayo3,
    "MAYO-3",
    32,
    2986,
    681,
    49147,
    mayo_3_seeded_keypair,
    mayo_3_expand_public_key,
    mayo_3_sign,
    mayo_3_verify,
    mayo_3_verify_expanded
);
parameter_set!(
    Mayo5,
    "MAYO-5",
    40,
    5554,
    964,
    107415,
    mayo_5_seeded_keypair,
    mayo_5_expand_public_key,
    mayo_5_sign,
    mayo_5_verify,
    mayo_5_verify_expanded
);

pub(crate) mod private {
    pub trait Sealed {
        const SEEDED_KEYPAIR: unsafe extern "C" fn(*mut u8, *const u8) -> i32;
        const EXPANDED_PUBLIC_KEY_WORDS: usize;
        const EXPAND_PUBLIC_KEY: unsafe extern "C" fn(*mut u64, *const u8) -> i32;
        const SIGN: unsafe extern "C" fn(*mut u8, *mut usize, *const u8, usize, *const u8) -> i32;
        const VERIFY: unsafe extern "C" fn(*const u8, usize, *const u8, usize, *const u8) -> i32;
        const VERIFY_EXPANDED: unsafe extern "C" fn(
            *const u8,
            usize,
            *const u8,
            usize,
            *const u64,
        ) -> i32;
    }
}
