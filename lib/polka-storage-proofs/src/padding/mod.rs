
/// References:
/// * <https://github.com/filecoin-project/lotus/blob/471819bf1ef8a4d5c7c0476a38ce9f5e23c59bfc/lib/filler/filler.go#L9>
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L164>
/// * <https://github.com/filecoin-project/go-commp-utils/blob/master/zerocomm/zerocomm.go>
pub fn calculate_unpadded_piece_sizes(remaining_space: usize) -> Vec<usize> {
    let mut padded = remaining_space + remaining_space / 127;

    let pieces = padded.count_ones() as usize;
    let mut unpadded_piece_sizes: Vec<usize> = vec![];
    for _ in 0..pieces {
        let next = padded.trailing_zeros();
        let psize = 1 << next;

        padded ^= psize;

        unpadded_piece_sizes.push(psize - psize / 128);
    }

    unpadded_piece_sizes
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn sanity() {
        let x: Vec<usize> = vec![];
        let smallest_piece_size = 127;
        let biggest_unpadded_piece_size = 1016;
        assert_eq!(vec![127, 254, 508, 1016], calculate_unpadded_piece_sizes(2032 - smallest_piece_size));
        assert_eq!(vec![1016], calculate_unpadded_piece_sizes(2032 - biggest_unpadded_piece_size));
    }
}