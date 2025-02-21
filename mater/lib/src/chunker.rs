use bytes::{Bytes, BytesMut};
use futures::Stream;
use tokio::io::{AsyncRead, AsyncReadExt};

pub(crate) fn byte_stream_chunker<S>(
    mut source: S,
    chunk_size: usize,
) -> impl Stream<Item = std::io::Result<Bytes>>
where
    S: AsyncRead + Unpin,
{
    // This custom stream gathers incoming buffers into a single byte chunk of `chunk_size`
    // `tokio_util::io::ReaderStream` does a very similar thing, however, it does not attempt
    // to fill it's buffer before returning, voiding the whole promise of properly sized chunks
    // There is an alternative implementation (untested & uses unsafe) in the following GitHub Gist:
    // https://gist.github.com/jmg-duarte/f606410a5e0314d7b5cee959a240b2d8
    async_stream::try_stream! {
        let mut buf = BytesMut::with_capacity(chunk_size);

        loop {
            if buf.capacity() < chunk_size {
                // BytesMut::reserve *may* allocate more memory than requested to avoid further
                // allocations, while that's very helpful, it's also unpredictable.
                // If and when necessary, we can replace this with the following line:
                // std::mem::replace(buf, BytesMut::with_capacity(chunk_size)):

                // Reserve only the difference as the split may leave nothing, or something
                buf.reserve(chunk_size - buf.capacity());
            }

            // If the read length is 0, we *assume* we reached EOF
            // tokio's docs state that this does not mean we exhausted the reader,
            // as it may be able to return more bytes later, *however*,
            // this means there is no right way of knowing when the reader is fully exhausted!
            // If we need to support a case like that, we just need to track how many times
            // the reader returned 0 and break at a certain point
            if source.read_buf(&mut buf).await? == 0 {
                // EOF but there's still content to yield -> yield it
                loop {
                    // Due to the lack of guarantees on the resulting size of `BytesMut::reserve`
                    // the buffer may contain more than `chunk_size`,
                    // in that case we must yield the remaning complete chunks first
                    let chunk = match buf.len() {
                        0 => break,
                        len if len <= chunk_size => buf.split(),
                        _ => buf.split_to(chunk_size),
                    };
                    yield chunk.freeze();
                }
                break
            } else if buf.len() >= chunk_size {
                // The buffer may have a larger capacity than chunk_size due to reserve
                // this also means that our read may have read more bytes than we expected,
                // thats why we check if the length if bigger than the chunk_size and if so
                // we split the buffer to the chunk_size, then freeze and return
                let chunk = buf.split_to(chunk_size);
                yield chunk.freeze();
            } // otherwise, the buffer is not full, so we don't do a thing
        }
    }
}
