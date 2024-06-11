use std::io::Cursor;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use mater::Blockstore;
use tokio::{fs::File, runtime::Runtime as TokioExecutor};

// Read content to a Blockstore. This function is benchmarked.
async fn read_content(content: &[u8], mut store: Blockstore) {
    let cursor = Cursor::new(content);
    store.read(cursor).await.unwrap()
}

fn read_file(c: &mut Criterion) {
    let paths = [
        "tests/fixtures/original/lorem.txt",
        "tests/fixtures/original/lorem_1024.txt",
        "tests/fixtures/original/lorem_4096_dup.txt",
        "tests/fixtures/original/spaceglenda.jpg",
    ];

    for path in paths.iter() {
        let content = std::fs::read(path).unwrap();

        c.bench_with_input(
            BenchmarkId::new("read_file", path),
            &content,
            |b, contents| {
                b.to_async(TokioExecutor::new().unwrap())
                    .iter(|| read_content(&contents, Blockstore::new()));
            },
        );
    }
}

// Write content from a Blockstore. This function is benchmarked.
async fn write_contents(buffer: Vec<u8>, store: Blockstore) {
    store.write(buffer).await.unwrap();
}

fn write_file(c: &mut Criterion) {
    let paths = [
        "tests/fixtures/original/lorem.txt",
        "tests/fixtures/original/lorem_1024.txt",
        "tests/fixtures/original/lorem_4096_dup.txt",
        "tests/fixtures/original/spaceglenda.jpg",
    ];

    let runtime = TokioExecutor::new().unwrap();
    for path in paths.iter() {
        let mut blockstore = Blockstore::new();

        // Read file contents to the blockstore
        runtime.block_on(async {
            let file = File::open(path).await.unwrap();
            blockstore.read(file).await.unwrap()
        });

        c.bench_with_input(BenchmarkId::new("write_file", path), &(), |b, _: &()| {
            b.to_async(TokioExecutor::new().unwrap()).iter_batched(
                || (blockstore.clone(), Vec::with_capacity(1024 * 1000)),
                |(blockstore, buffer)| write_contents(buffer, blockstore),
                BatchSize::SmallInput,
            );
        });
    }
}

criterion_group!(bench_reading, read_file);
criterion_group!(bench_writing, write_file);
criterion_main!(bench_reading, bench_writing);
