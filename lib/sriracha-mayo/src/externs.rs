macro_rules! externs {
    ($backend:literal) => {
        // SAFETY: These declarations bind the MAYO-C NIST API symbols emitted
        // by the selected backend. Safe wrappers enforce buffer sizes and
        // pointer lifetimes before calling them.
        unsafe extern "C" {
            #[link_name = concat!("pqmayo_MAYO_1_", $backend, "_seeded_keypair")]
            pub(crate) fn mayo_1_seeded_keypair(pk: *mut u8, sk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_1_", $backend, "_expand_public_key")]
            pub(crate) fn mayo_1_expand_public_key(pk: *mut u64, cpk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_1_", $backend, "_crypto_sign_signature")]
            pub(crate) fn mayo_1_sign(
                sig: *mut u8,
                siglen: *mut usize,
                message: *const u8,
                message_len: usize,
                sk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_1_", $backend, "_crypto_sign_verify")]
            pub(crate) fn mayo_1_verify(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_1_", $backend, "_verify_expanded")]
            pub(crate) fn mayo_1_verify_expanded(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u64,
            ) -> i32;

            #[link_name = concat!("pqmayo_MAYO_2_", $backend, "_seeded_keypair")]
            pub(crate) fn mayo_2_seeded_keypair(pk: *mut u8, sk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_2_", $backend, "_expand_public_key")]
            pub(crate) fn mayo_2_expand_public_key(pk: *mut u64, cpk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_2_", $backend, "_crypto_sign_signature")]
            pub(crate) fn mayo_2_sign(
                sig: *mut u8,
                siglen: *mut usize,
                message: *const u8,
                message_len: usize,
                sk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_2_", $backend, "_crypto_sign_verify")]
            pub(crate) fn mayo_2_verify(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_2_", $backend, "_verify_expanded")]
            pub(crate) fn mayo_2_verify_expanded(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u64,
            ) -> i32;

            #[link_name = concat!("pqmayo_MAYO_3_", $backend, "_seeded_keypair")]
            pub(crate) fn mayo_3_seeded_keypair(pk: *mut u8, sk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_3_", $backend, "_expand_public_key")]
            pub(crate) fn mayo_3_expand_public_key(pk: *mut u64, cpk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_3_", $backend, "_crypto_sign_signature")]
            pub(crate) fn mayo_3_sign(
                sig: *mut u8,
                siglen: *mut usize,
                message: *const u8,
                message_len: usize,
                sk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_3_", $backend, "_crypto_sign_verify")]
            pub(crate) fn mayo_3_verify(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_3_", $backend, "_verify_expanded")]
            pub(crate) fn mayo_3_verify_expanded(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u64,
            ) -> i32;

            #[link_name = concat!("pqmayo_MAYO_5_", $backend, "_seeded_keypair")]
            pub(crate) fn mayo_5_seeded_keypair(pk: *mut u8, sk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_5_", $backend, "_expand_public_key")]
            pub(crate) fn mayo_5_expand_public_key(pk: *mut u64, cpk: *const u8) -> i32;
            #[link_name = concat!("pqmayo_MAYO_5_", $backend, "_crypto_sign_signature")]
            pub(crate) fn mayo_5_sign(
                sig: *mut u8,
                siglen: *mut usize,
                message: *const u8,
                message_len: usize,
                sk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_5_", $backend, "_crypto_sign_verify")]
            pub(crate) fn mayo_5_verify(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u8,
            ) -> i32;
            #[link_name = concat!("pqmayo_MAYO_5_", $backend, "_verify_expanded")]
            pub(crate) fn mayo_5_verify_expanded(
                sig: *const u8,
                siglen: usize,
                message: *const u8,
                message_len: usize,
                pk: *const u64,
            ) -> i32;
        }
    };
}

#[cfg(mayo_backend_avx2)]
externs!("avx2");

#[cfg(mayo_backend_neon)]
externs!("neon");

#[cfg(mayo_backend_opt)]
externs!("opt");
