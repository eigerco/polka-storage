use std::{
    fmt::Display,
    io::Cursor,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use mater::FileWriter;
use rand::{prelude::SliceRandom, rngs::ThreadRng, Rng};
use tempfile::{tempdir, TempDir};
use tokio::{
    fs::File,
    io::{AsyncSeek, AsyncWrite},
    runtime::Runtime as TokioExecutor,
};

static FILES: OnceLock<Vec<(Params, PathBuf, TempDir)>> = OnceLock::new();
fn get_source_files() -> &'static Vec<(Params, PathBuf, TempDir)> {
    FILES.get_or_init(|| {
        let params = get_params();
        let mut contents = vec![];

        for param in params {
            // Prepare temporary files
            let content = generate_content(&param);
            let (temp_dir, source_file) = prepare_source_file(&content);

            contents.push((param, source_file, temp_dir));
        }

        contents
    })
}

/// Get content sizes for the benchmarks.
const SIZES: [usize; 3] = [
    1024 * 10000,   // 10 MB
    1024 * 100000,  // 100 MB
    1024 * 1000000, // 1 GB
];

/// The percentage of duplicated content in the file. e.g. 0.8 means 80% of the file is duplicated.
const DUPLICATIONS: [f64; 5] = [0.0, 0.1, 0.2, 0.4, 0.8];

/// The default block size
const BLOCK_SIZE: usize = 1024 * 256;

/// A chunk of data
#[derive(Debug, Clone, Copy)]
struct Chunk([u8; BLOCK_SIZE]);

impl Chunk {
    fn new_random(rng: &mut ThreadRng) -> Self {
        Self([0; BLOCK_SIZE].map(|_| rng.gen()))
    }

    fn new_zeroed() -> Self {
        Self([0; BLOCK_SIZE])
    }
}

impl IntoIterator for Chunk {
    type Item = u8;
    type IntoIter = std::array::IntoIter<u8, BLOCK_SIZE>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone, Copy)]
struct Params {
    /// The size of the content in bytes.
    size: usize,
    /// The percentage of duplicated content in the file.
    duplication: f64,
}

impl Display for Params {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "content_size: {} bytes, percent_duplicated: {}",
            self.size, self.duplication
        )
    }
}

/// Get combination of parameters for the benchmarks.
fn get_params() -> Vec<Params> {
    SIZES
        .iter()
        .flat_map(|&size| {
            DUPLICATIONS
                .iter()
                .map(move |&duplication| Params { size, duplication })
        })
        .collect()
}

/// Generate content for the benchmarks. The duplicated data is placed between
/// the random chunks.
fn generate_content(params: &Params) -> Vec<u8> {
    let num_chunks = params.size / BLOCK_SIZE;
    let mut chunks = Vec::with_capacity(num_chunks);
    let mut rng = rand::thread_rng();

    // Generate zeroed chunks for the specified percentage of the content. Other
    // part is filled with random chunks.
    for index in 1..=num_chunks {
        let percentage_processed = index as f64 / num_chunks as f64;
        if percentage_processed < params.duplication {
            chunks.push(Chunk::new_zeroed());
        } else {
            chunks.push(Chunk::new_random(&mut rng));
        }
    }

    // Shuffle the chunks
    chunks.shuffle(&mut rng);

    // Flatten the chunks into a single byte array
    let mut bytes = chunks.into_iter().flatten().collect::<Vec<u8>>();

    // There can be some bytes missing because we are generating data in chunks.
    // We append the random data at the end.
    let missing_bytes_len = params.size - bytes.len();
    bytes.extend(
        (0..missing_bytes_len)
            .map(|_| rng.gen())
            .collect::<Vec<u8>>(),
    );

    bytes
}

/// Read/Write content to a Blockstore. This function is benchmarked.
async fn read_write_content_benched<W>(content: &[u8], mut store: FileWriter<W>)
where
    W: AsyncWrite + AsyncSeek + Unpin,
{
    let cursor = Cursor::new(content);
    store.write_from(cursor).await.unwrap();
    store.finish().await.unwrap();
}

fn read_write(c: &mut Criterion) {
    let files = get_source_files();

    for (params, source_file, _) in files {
        let content = std::fs::read(&source_file).unwrap();

        c.bench_with_input(BenchmarkId::new("read", params), params, |b, _params| {
            b.to_async(TokioExecutor::new().unwrap())
                .iter(|| read_write_content_benched(&content, FileWriter::in_memory()));
        });
    }
}

/// Prepare temporary file
fn prepare_source_file(content: &[u8]) -> (TempDir, PathBuf) {
    let temp_dir = tempdir().unwrap();
    let file = temp_dir.path().join("source_file");

    // Write content to the file
    std::fs::write(&file, &content).unwrap();

    (temp_dir, file)
}

/// Import a source into a writer. This function is benchmarked.
async fn blockwriter_import(source: &Path, target: &Path) {
    let source_file = File::open(source).await.unwrap();
    let output_file = File::create(target).await.unwrap();

    FileWriter::import(source_file, output_file).await.unwrap();
}

fn import(c: &mut Criterion) {
    let files = get_source_files();

    for (params, source_file, temp_dir) in files {
        let target_file = temp_dir.path().join("target");

        c.bench_with_input(BenchmarkId::new("filestore", params), &(), |b, _: &()| {
            b.to_async(TokioExecutor::new().unwrap())
                .iter(|| blockwriter_import(&source_file, &target_file));
        });
    }
}

criterion_group!(bench_reading, read_write);
criterion_group!(bench_import, import);
criterion_main!(bench_reading, bench_import);
