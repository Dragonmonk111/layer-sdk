use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use sriracha_mayo::{BatchVerifier, Mayo1, Mayo2, Mayo3, Mayo5, ParameterSet, SecretKey};
use std::hint::black_box;

const MESSAGE: &[u8] = b"MAYO Criterion benchmark message";
const BATCH_SIZE: usize = 128;

fn benchmark_variant<P: ParameterSet + 'static>(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group(P::NAME);

    group.bench_function("keygen", |bencher| {
        let mut rng = ChaCha20Rng::from_seed([1; 32]);
        bencher.iter(|| {
            let keypair = SecretKey::<P>::random(&mut rng).unwrap();
            black_box(keypair);
        });
    });

    let mut signing_rng = ChaCha20Rng::from_seed([2; 32]);
    let (_, secret_key) = SecretKey::<P>::random(&mut signing_rng).unwrap();
    group.bench_function("sign", |bencher| {
        bencher.iter(|| {
            let signature = secret_key.sign(black_box(MESSAGE)).unwrap();
            black_box(signature);
        });
    });

    let mut verify_rng = ChaCha20Rng::from_seed([3; 32]);
    let (public_key, secret_key) = SecretKey::<P>::random(&mut verify_rng).unwrap();
    let signature = secret_key.sign(MESSAGE).unwrap();
    assert!(signature.verify(&public_key, MESSAGE));

    group.bench_function("verify", |bencher| {
        bencher.iter(|| {
            let verified = signature.verify(&public_key, black_box(MESSAGE));
            black_box(verified);
        });
    });

    let (public_key, messages, signatures) = signed_batch::<P>();
    group.bench_function("verify_128", |bencher| {
        bencher.iter(|| {
            for (message, signature) in messages.iter().zip(signatures.iter()) {
                let verified = signature.verify(&public_key, black_box(message.as_ref()));
                black_box(verified);
            }
        });
    });

    let mut batch = BatchVerifier::new();
    for (message, signature) in messages.iter().zip(signatures.iter()) {
        batch.add(&public_key, message, signature).unwrap();
    }
    assert!(batch.verify());

    group.bench_function("batch_add_verify_128", |bencher| {
        bencher.iter(|| {
            let mut batch = BatchVerifier::new();
            for (message, signature) in messages.iter().zip(signatures.iter()) {
                batch.add(&public_key, message, signature).unwrap();
            }

            let verified = batch.verify();
            black_box(verified);
        });
    });

    group.finish();
}

fn signed_batch<P: ParameterSet>() -> (
    sriracha_mayo::PublicKey<P>,
    Vec<Bytes>,
    Vec<sriracha_mayo::Signature<P>>,
) {
    let mut rng = ChaCha20Rng::from_seed([4; 32]);
    let (public_key, secret_key) = SecretKey::<P>::random(&mut rng).unwrap();
    let mut messages = Vec::with_capacity(BATCH_SIZE);
    let mut signatures = Vec::with_capacity(BATCH_SIZE);

    for index in 0..BATCH_SIZE {
        let message = Bytes::from(format!("MAYO Criterion batch message {index}"));
        let signature = secret_key.sign(&message).unwrap();

        messages.push(message);
        signatures.push(signature);
    }

    (public_key, messages, signatures)
}

fn benchmarks(criterion: &mut Criterion) {
    benchmark_variant::<Mayo1>(criterion);
    benchmark_variant::<Mayo2>(criterion);
    benchmark_variant::<Mayo3>(criterion);
    benchmark_variant::<Mayo5>(criterion);
}

criterion_group!(mayo, benchmarks);
criterion_main!(mayo);
