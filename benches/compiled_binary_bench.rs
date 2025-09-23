use criterion::{Criterion, criterion_group, criterion_main};

fn bench_aot_binary(c: &mut Criterion) {
    let mut time_sum = 0;
    let mut time_count = 0;

    c.bench_function("compiled_binary", |b| {
        b.iter(|| {
            let status = std::process::Command::new("./build/aot-test")
                .stdout(std::process::Stdio::null())
                .status()
                .unwrap();

            time_sum += status.code().unwrap();
            time_count += 1;
        });
    });

    println!("Average time: {} mcs", time_sum as f64 /time_count as f64 )
}

criterion_group!(benches, bench_aot_binary);
criterion_main!(benches);
