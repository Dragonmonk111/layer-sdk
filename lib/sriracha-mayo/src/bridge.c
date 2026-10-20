#include <mayo.h>

#undef mayo_keypair
#undef mayo_sign_signature
#undef mayo_sign
#undef mayo_open
#undef mayo_keypair_compact
#undef mayo_expand_pk
#undef mayo_expand_sk
#undef mayo_verify

#define mayo_keypair MAYO_NAMESPACE(rust_hidden_mayo_keypair)
#define mayo_sign_signature MAYO_NAMESPACE(rust_hidden_mayo_sign_signature)
#define mayo_sign MAYO_NAMESPACE(rust_hidden_mayo_sign)
#define mayo_open MAYO_NAMESPACE(rust_hidden_mayo_open)
#define mayo_keypair_compact MAYO_NAMESPACE(rust_hidden_mayo_keypair_compact)
#define mayo_expand_pk MAYO_NAMESPACE(rust_hidden_mayo_expand_pk)
#define mayo_expand_sk MAYO_NAMESPACE(rust_hidden_mayo_expand_sk)
#define mayo_verify MAYO_NAMESPACE(rust_hidden_mayo_verify)

int mayo_keypair_compact(const mayo_params_t *p, unsigned char *cpk, unsigned char *csk);
int mayo_verify(const mayo_params_t *p, const unsigned char *m, size_t mlen,
                const unsigned char *sig, const unsigned char *cpk);

#include "mayo.c"

#define mayo_seeded_keypair MAYO_NAMESPACE(seeded_keypair)
#define mayo_expand_public_key MAYO_NAMESPACE(expand_public_key)
#define mayo_verify_expanded MAYO_NAMESPACE(verify_expanded)

int mayo_expand_public_key(uint64_t *pk, const unsigned char *cpk) {
    int ret = mayo_expand_pk(0, cpk, pk);
    if (ret != MAYO_OK) {
        return MAYO_ERR;
    }

#ifdef TARGET_BIG_ENDIAN
    uint64_t *P1 = pk;
    uint64_t *P2 = P1 + PARAM_P1_limbs(0);
    uint64_t *P3 = P2 + PARAM_P2_limbs(0);

    for (int i = 0; i < PARAM_P1_limbs(0); ++i) {
        P1[i] = BSWAP64(P1[i]);
    }
    for (int i = 0; i < PARAM_P2_limbs(0); ++i) {
        P2[i] = BSWAP64(P2[i]);
    }
    for (int i = 0; i < PARAM_P3_limbs(0); ++i) {
        P3[i] = BSWAP64(P3[i]);
    }
#endif

    return MAYO_OK;
}

int mayo_verify_expanded(const unsigned char *sig, size_t siglen,
                         const unsigned char *m, size_t mlen,
                         const uint64_t *pk) {
    if (siglen != PARAM_sig_bytes(0)) {
        return MAYO_ERR;
    }

    unsigned char tEnc[M_BYTES_MAX];
    unsigned char t[M_MAX];
    unsigned char y[2 * M_MAX] = {0};
    unsigned char s[K_MAX * N_MAX];
    unsigned char tmp[DIGEST_BYTES_MAX + SALT_BYTES_MAX];

    const int param_m = PARAM_m(0);
    const int param_n = PARAM_n(0);
    const int param_k = PARAM_k(0);
    const int param_m_bytes = PARAM_m_bytes(0);
    const int param_sig_bytes = PARAM_sig_bytes(0);
    const int param_digest_bytes = PARAM_digest_bytes(0);
    const int param_salt_bytes = PARAM_salt_bytes(0);

    const uint64_t *P1 = pk;
    const uint64_t *P2 = P1 + PARAM_P1_limbs(0);
    const uint64_t *P3 = P2 + PARAM_P2_limbs(0);

    shake256(tmp, param_digest_bytes, m, mlen);

    memcpy(tmp + param_digest_bytes, sig + param_sig_bytes - param_salt_bytes,
           param_salt_bytes);
    shake256(tEnc, param_m_bytes, tmp, param_digest_bytes + param_salt_bytes);
    decode(tEnc, t, param_m);

    decode(sig, s, param_k * param_n);
    eval_public_map(0, s, P1, P2, P3, y);

    if (memcmp(y, t, param_m) == 0) {
        return MAYO_OK;
    }

    return MAYO_ERR;
}

int mayo_seeded_keypair(unsigned char *cpk, const unsigned char *csk) {
    unsigned char S[PK_SEED_BYTES_MAX + O_BYTES_MAX];
    uint64_t P[P1_LIMBS_MAX + P2_LIMBS_MAX];
    uint64_t P3[O_MAX * O_MAX * M_VEC_LIMBS_MAX] = {0};
    unsigned char O[V_MAX * O_MAX];

    const int m_vec_limbs = PARAM_m_vec_limbs(0);
    const int param_m = PARAM_m(0);
    const int param_v = PARAM_v(0);
    const int param_o = PARAM_o(0);
    const int param_O_bytes = PARAM_O_bytes(0);
    const int param_P1_limbs = PARAM_P1_limbs(0);
    const int param_P3_limbs = PARAM_P3_limbs(0);
    const int param_pk_seed_bytes = PARAM_pk_seed_bytes(0);
    const int param_sk_seed_bytes = PARAM_sk_seed_bytes(0);

    uint64_t *P1 = P;
    uint64_t *P2 = P + param_P1_limbs;

    shake256(S, param_pk_seed_bytes + param_O_bytes, csk, param_sk_seed_bytes);
    unsigned char *seed_pk = S;

    decode(S + param_pk_seed_bytes, O, param_v * param_o);
    expand_P1_P2(0, P, seed_pk);
    compute_P3(0, P1, P2, O, P3);

    memcpy(cpk, seed_pk, param_pk_seed_bytes);

    uint64_t P3_upper[P3_LIMBS_MAX];
    m_upper(0, P3, P3_upper, param_o);
    pack_m_vecs(P3_upper, cpk + param_pk_seed_bytes, param_P3_limbs / m_vec_limbs, param_m);

    mayo_secure_clear(S, sizeof(S));
    mayo_secure_clear(O, sizeof(O));
    mayo_secure_clear(P2, PARAM_P2_limbs(0) * sizeof(uint64_t));
    mayo_secure_clear(P3, sizeof(P3));
    mayo_secure_clear(P3_upper, sizeof(P3_upper));

    return MAYO_OK;
}
