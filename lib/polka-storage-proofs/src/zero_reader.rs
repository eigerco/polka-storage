use std::io::Read;

/// Reader that returns zeros if the inner reader is empty.
pub struct ZeroPaddingReader<R: Read> {
    /// The inner reader to read from.
    inner: R,
    /// The number of bytes this 0-padding reader has left to produce.
    remaining: u64,
}

impl<R: Read> ZeroPaddingReader<R> {
    pub fn new(inner: R, total_size: u64) -> Self {
        Self {
            inner,
            remaining: total_size,
        }
    }
}

impl<R: Read> Read for ZeroPaddingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }

        // Number of bytes that the reader will produce in this execution
        let to_read = buf.len().min(self.remaining as usize);
        // Number of bytes that we read from the inner reader
        let read = self.inner.read(&mut buf[..to_read])?;

        // Incomplete read doesn't mean that we need to pad it yet.
        // Next call can be complete.
        if read > 0 {
            self.remaining -= read as u64;
            return Ok(read);
        }

        // If we read from the inner reader less then the required bytes, 0-pad
        // the rest of the buffer.
        buf[..to_read].fill(0);

        // Decrease the number of bytes this 0-padding reader has left to produce.
        self.remaining -= to_read as u64;

        // Return the number of bytes that we wrote to the buffer.
        Ok(to_read)
    }
}

#[cfg(test)]
mod tests {

    use std::io::Read;

    use super::ZeroPaddingReader;

    /// Sole purpose of this reader is to simulate the file reading in the OS.
    /// When we give it a buf of a certain buf.length(), it might read n < buf.length().
    struct IncompleteReader {
        read_count: usize,
        served_data: Vec<Vec<u8>>,
    }

    impl Read for IncompleteReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.read_count == self.served_data.len() {
                return Ok(0);
            }

            let ready = &self.served_data[self.read_count];
            // this is a simplification for test purposes
            // assuming buf.len() >= ready.len() always holds
            buf[..ready.len()].copy_from_slice(&ready);

            self.read_count += 1;
            Ok(ready.len())
        }
    }

    #[test]
    fn test_zero_padding_reader_with_not_full_reads() {
        let r = IncompleteReader {
            read_count: 0,
            served_data: vec![vec![1, 2, 3, 4], vec![5, 6], vec![7, 8, 9], vec![10]],
        };
        let total_size_with_padding = 12;
        let mut reader = ZeroPaddingReader::new(r, total_size_with_padding);
        let mut buffer = [0; 4];

        let mut total_read = 0;
        let read = reader.read(&mut buffer).unwrap();
        total_read += read;
        assert_eq!(read, 4);
        assert_eq!(buffer, [1, 2, 3, 4]);

        let read = reader.read(&mut buffer).unwrap();
        total_read += read;
        assert_eq!(read, 2);
        assert_eq!(buffer, [5, 6, 3, 4]);

        let read = reader.read(&mut buffer).unwrap();
        total_read += read;
        assert_eq!(read, 3);
        assert_eq!(buffer, [7, 8, 9, 4]);

        let read = reader.read(&mut buffer).unwrap();
        total_read += read;
        assert_eq!(read, 1);
        assert_eq!(buffer, [10, 8, 9, 4]);

        let read = reader.read(&mut buffer).unwrap();
        total_read += read;
        assert_eq!(read, 2);
        assert_eq!(buffer, [0, 0, 9, 4]);

        assert_eq!(total_size_with_padding as usize, total_read);
    }

    #[test]
    fn test_zero_padding_reader() {
        let data = vec![1, 2, 3, 4, 5, 6];
        let total_size = 10;
        let mut reader = ZeroPaddingReader::new(&data[..], total_size);

        let mut buffer = [0; 4];
        // First read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 4);
        assert_eq!(buffer, [1, 2, 3, 4]);
        // Second read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 2);
        assert_eq!(buffer, [5, 6, 3, 4]);
        // Third read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 4);
        assert_eq!(buffer, [0, 0, 0, 0]);
        // Fourth read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 0);
        assert_eq!(buffer, [0, 0, 0, 0]);
    }
}
