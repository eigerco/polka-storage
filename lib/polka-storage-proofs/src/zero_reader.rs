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

        // If we read from the inner reader less then the required bytes, 0-pad
        // the rest of the buffer.
        if read < to_read {
            buf[read..to_read].fill(0);
        }

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
        assert_eq!(read, 4);
        assert_eq!(buffer, [5, 6, 0, 0]);
        // Third read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 2);
        assert_eq!(buffer, [0, 0, 0, 0]);
        // Fourth read
        let read = reader.read(&mut buffer).unwrap();
        assert_eq!(read, 0);
        assert_eq!(buffer, [0, 0, 0, 0]);
    }
}
