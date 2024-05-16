use std::io::{self, BufRead};

use fallible_iterator::FallibleIterator;

struct FixedChunker<R: BufRead> {
    reader: R,
    buffer: Option<Vec<u8>>,
}

impl<R: BufRead> FixedChunker<R> {
    pub fn new(size: usize, reader: R) -> Self {
        Self {
            reader,
            buffer: Some(vec![0; size]),
        }
    }
}

impl<R: BufRead> FallibleIterator for FixedChunker<R> {
    type Item = (Vec<u8>, usize);
    type Error = io::Error;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        if let Some(inner) = &mut self.buffer {
            let n_read = self.reader.read(inner)?;

            match n_read {
                0 => return Ok(None),
                n if n == inner.len() => return Ok(Some((inner.to_vec(), n_read))),
                _ => {
                    if let Some(inner) = &mut self.buffer {
                        inner.truncate(n_read);
                    }
                    return Ok(self.buffer.take().map(|v| (v, n_read)));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::BufReader};

    use fallible_iterator::FallibleIterator;

    use super::FixedChunker;

    #[test]
    fn read_1024() {
        const CHUNK_SIZE: usize = 1024;
        const TEST_PATH: &str = "tests/fixtures/lorem_1024.txt";
        let read_file = std::fs::read(TEST_PATH).unwrap();
        assert_eq!(read_file.len(), CHUNK_SIZE); // sanity check

        let file = File::open(TEST_PATH).unwrap();
        let buffered_file = BufReader::new(file);
        let mut chunker = FixedChunker::new(CHUNK_SIZE, buffered_file);

        let (chunk, size) = chunker.next().unwrap().unwrap();
        assert_eq!(size, CHUNK_SIZE);
        assert_eq!(read_file, chunk);

        let chunk = chunker.next().unwrap();
        assert_eq!(chunk, None);
    }

    #[test]
    fn read_4096() {
        const CHUNK_SIZE: usize = 1024;
        const TEST_PATH: &str = "tests/fixtures/lorem_4096_dup.txt";
        let read_file = std::fs::read(TEST_PATH).unwrap();
        assert_eq!(read_file.len(), CHUNK_SIZE * 4); // sanity check

        let file = File::open(TEST_PATH).unwrap();
        let buffered_file = BufReader::new(file);
        let mut chunker = FixedChunker::new(CHUNK_SIZE, buffered_file);

        for idx in 0..4 {
            let (chunk, size) = chunker.next().unwrap().unwrap();
            assert_eq!(size, CHUNK_SIZE);
            assert_eq!(
                read_file[(CHUNK_SIZE * idx)..(CHUNK_SIZE * (idx + 1))],
                chunk
            );
        }

        let chunk = chunker.next().unwrap();
        assert_eq!(chunk, None);
    }

    #[test]
    fn read_7564() {
        const FILE_SIZE: usize = 7564;
        const CHUNK_SIZE: usize = 1024;
        const TEST_PATH: &str = "tests/fixtures/lorem.txt";
        let read_file = std::fs::read(TEST_PATH).unwrap();
        assert_eq!(read_file.len(), FILE_SIZE); // sanity check

        let file = File::open(TEST_PATH).unwrap();
        let buffered_file = BufReader::new(file);
        let mut chunker = FixedChunker::new(CHUNK_SIZE, buffered_file);

        let expected_n_chunks = FILE_SIZE / CHUNK_SIZE; // this is actually 7
        for idx in 0..expected_n_chunks {
            let (chunk, size) = chunker.next().unwrap().unwrap();
            assert_eq!(size, CHUNK_SIZE);
            assert_eq!(
                read_file[(CHUNK_SIZE * idx)..(CHUNK_SIZE * (idx + 1))],
                chunk
            );
        }

        let (chunk, size) = chunker.next().unwrap().unwrap();
        assert_eq!(size, FILE_SIZE - (expected_n_chunks * CHUNK_SIZE));
        assert_eq!(read_file[7 * CHUNK_SIZE..], chunk);

        let chunk = chunker.next().unwrap();
        assert_eq!(chunk, None);
    }
}
