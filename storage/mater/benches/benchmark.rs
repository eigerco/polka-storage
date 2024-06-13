use std::{
    fmt::Display,
    io::Cursor,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use mater::{create_filestore, Blockstore, Config};
use tempfile::{tempdir, TempDir};
use tokio::runtime::Runtime as TokioExecutor;

#[derive(Debug, Clone, Copy)]
struct Params {
    size: usize,
    num: usize,
}

impl Display for Params {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "content_size: {} bytes, num_of_duplicates: {}",
            self.size, self.num
        )
    }
}

/// Get content sizes for the benchmarks.
fn get_sizes() -> Vec<usize> {
    vec![
        1024 * 1000,    // 1 MB
        1024 * 10000,   // 10 MB
        1024 * 100000,  // 100 MB
        1024 * 1000000, // 1 GB
    ]
}

/// Get number of copies for the benchmarks. Zero means that there are no copies
/// and the whole content is unique.
fn get_num_copies() -> Vec<usize> {
    vec![0, 1, 2, 4]
}

static CONTENTS: OnceLock<Vec<(Params, Vec<u8>)>> = OnceLock::new();
fn get_contents() -> &'static Vec<(Params, Vec<u8>)> {
    CONTENTS.get_or_init(|| {
        let mut contents = vec![];
        for size in get_sizes() {
            for num in get_num_copies() {
                let content = create_content(size, num);
                contents.push((Params { size, num }, content));
            }
        }

        contents
    })
}

/// Create random content of a given size. Duplicates are used to specify how
/// many times the content should be repeated.
fn create_content(size: usize, num_of_copies: usize) -> Vec<u8> {
    let single_part_size = size / (num_of_copies + 1);
    let single_content = (0..single_part_size)
        .map(|_| rand::random())
        .collect::<Vec<u8>>();

    single_content.repeat(num_of_copies)
}

/// Prepare temporary file
fn prepare_source_file(content: &[u8]) -> (TempDir, PathBuf) {
    let temp_dir = tempdir().unwrap();
    let file = temp_dir.path().join("source_file");

    // Write content to the file
    std::fs::write(&file, &content).unwrap();

    (temp_dir, file)
}

/// Read content to a Blockstore. This function is benchmarked.
async fn read_content_benched(content: &[u8], mut store: Blockstore) {
    let cursor = Cursor::new(content);
    store.read(cursor).await.unwrap()
}

fn read(c: &mut Criterion) {
    let contents = get_contents();

    for (params, content) in contents {
        c.bench_with_input(BenchmarkId::new("read", params), params, |b, _params| {
            b.to_async(TokioExecutor::new().unwrap())
                .iter(|| read_content_benched(&content, Blockstore::new()));
        });
    }
}

/// Write content from a Blockstore. This function is benchmarked.
async fn write_contents_benched(buffer: Vec<u8>, store: Blockstore) {
    store.write(buffer).await.unwrap();
}

fn write(c: &mut Criterion) {
    let runtime = TokioExecutor::new().unwrap();
    let contents = get_contents();

    for (params, content) in contents {
        let mut blockstore = Blockstore::new();

        // Read file contents to the blockstore
        runtime.block_on(async {
            let cursor = Cursor::new(content);
            blockstore.read(cursor).await.unwrap()
        });

        c.bench_with_input(BenchmarkId::new("write", params), &(), |b, _: &()| {
            b.to_async(TokioExecutor::new().unwrap()).iter_batched(
                || (blockstore.clone(), Vec::with_capacity(params.size)),
                |(blockstore, buffer)| write_contents_benched(buffer, blockstore),
                BatchSize::SmallInput,
            );
        });
    }
}

/// Create a filestore. This function is benchmarked.
async fn create_filestore_benched(source: &Path, target: &Path) {
    create_filestore(source, target, Config::default())
        .await
        .unwrap();
}

fn filestore(c: &mut Criterion) {
    let contents = get_contents();

    for (params, content) in contents {
        // Prepare temporary files
        let (temp_dir, source_file) = prepare_source_file(&content);
        let target_file = temp_dir.path().join("target");

        c.bench_with_input(BenchmarkId::new("filestore", params), &(), |b, _: &()| {
            b.to_async(TokioExecutor::new().unwrap())
                .iter(|| create_filestore_benched(&source_file, &target_file));
        });
    }
}

criterion_group!(bench_reading, read);
criterion_group!(bench_writing, write);
criterion_group!(bench_filestore, filestore);
criterion_main!(bench_reading, bench_writing, bench_filestore);
