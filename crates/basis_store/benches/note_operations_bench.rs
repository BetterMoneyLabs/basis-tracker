use basis_store::{
    schnorr::{self, generate_keypair},
    IouNote,
};
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_note_creation(c: &mut Criterion) {
    c.bench_function("create_and_sign_note", |b| {
        let (secret, _) = generate_keypair();
        let recipient_pubkey = [2u8; 33];

        b.iter(|| {
            let note = IouNote::create_and_sign(
                black_box(recipient_pubkey),
                black_box(1000),
                black_box(1234567890),
                black_box(&secret.secret_bytes()),
            );
            black_box(note);
        });
    });
}

fn bench_signature_verification(c: &mut Criterion) {
    c.bench_function("verify_note_signature", |b| {
        let (secret, issuer_pubkey) = generate_keypair();
        let recipient_pubkey = [2u8; 33];

        let note =
            IouNote::create_and_sign(recipient_pubkey, 1000, 1234567890, &secret.secret_bytes())
                .unwrap();

        b.iter(|| {
            let result = note.verify_signature(black_box(&issuer_pubkey));
            black_box(result);
        });
    });
}

fn bench_schnorr_signature(c: &mut Criterion) {
    c.bench_function("schnorr_sign", |b| {
        let (secret, pubkey) = generate_keypair();
        let message = b"benchmark message for schnorr signing";

        b.iter(|| {
            let signature = schnorr::schnorr_sign(
                black_box(message),
                black_box(&secret.into()),
                black_box(&pubkey),
            );
            black_box(signature);
        });
    });

    c.bench_function("schnorr_verify", |b| {
        let (secret, pubkey) = generate_keypair();
        let message = b"benchmark message for schnorr verification";

        let signature = schnorr::schnorr_sign(message, &secret.into(), &pubkey).unwrap();

        b.iter(|| {
            let result = schnorr::schnorr_verify(
                black_box(&signature),
                black_box(message),
                black_box(&pubkey),
            );
            black_box(result);
        });
    });
}

fn bench_note_serialization(c: &mut Criterion) {
    c.bench_function("note_field_access", |b| {
        let note = IouNote::new([1u8; 33], 1000, 0, 1234567890, [2u8; 65]);

        b.iter(|| {
            let recipient = black_box(note.recipient_pubkey);
            let amount = black_box(note.amount_collected);
            let redeemed = black_box(note.amount_redeemed);
            let timestamp = black_box(note.timestamp);
            let signature = black_box(note.signature);

            black_box((recipient, amount, redeemed, timestamp, signature));
        });
    });
}

fn bench_outstanding_debt_calculation(c: &mut Criterion) {
    c.bench_function("calculate_outstanding_debt", |b| {
        let note = IouNote::new([1u8; 33], 1000, 250, 1234567890, [2u8; 65]);

        b.iter(|| {
            let debt = black_box(note.outstanding_debt());
            let is_fully_redeemed = black_box(note.is_fully_redeemed());
            black_box((debt, is_fully_redeemed));
        });
    });
}

fn bench_bulk_note_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_operations");

    for count in [10, 100, 1000].iter() {
        group.bench_with_input(format!("create_{}_notes", count), count, |b, &count| {
            let (secret, _) = generate_keypair();

            b.iter(|| {
                let mut notes = Vec::with_capacity(count);
                for i in 0..count {
                    let note = IouNote::create_and_sign(
                        [i as u8; 33],
                        1000 + i as u64,
                        1234567890 + i as u64,
                        &secret.secret_bytes(),
                    )
                    .unwrap();
                    notes.push(black_box(note));
                }
                black_box(notes);
            });
        });
    }

    group.finish();
}

fn bench_hash_operations(c: &mut Criterion) {
    c.bench_function("blake2b256_hash", |b| {
        let data = vec![1u8; 64];

        b.iter(|| {
            use basis_store::blake2b256_hash;
            let hash = blake2b256_hash(black_box(&data));
            black_box(hash);
        });
    });
}

criterion_group!(
    benches,
    bench_note_creation,
    bench_signature_verification,
    bench_schnorr_signature,
    bench_note_serialization,
    bench_outstanding_debt_calculation,
    bench_bulk_note_operations,
    bench_hash_operations
);
criterion_main!(benches);
