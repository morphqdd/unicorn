use criterion::{Criterion, criterion_group, criterion_main};

fn bench_aot_binary(c: &mut Criterion) {
    c.bench_function("compiled_binary", |b| {
        b.iter(|| {
            for _ in 0..100 {
                let status = std::process::Command::new("./build/aot-test")
                    .stdout(std::process::Stdio::null())
                    .status()
                    .unwrap();
                assert_eq!(status.code(), Some(20));
            }
        });
    });
}

criterion_group!(benches, bench_aot_binary);
criterion_main!(benches);
