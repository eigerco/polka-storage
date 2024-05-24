use codec::{Decode, Encode, Error as CodecError};

pub mod address;
pub mod registered_proof;

/// Identifier for Actors, includes builtin and initialized actors
pub type ActorID = u64;

/// Identifier for a CID
pub type Cid = String;

#[derive(Decode, Encode, Default)]
pub struct MinerId(pub u32);

// Code from https://github.com/paritytech/polkadot/blob/rococo-v1/parachain/src/primitives.rs
/// This type can be converted into and possibly from an AccountId (which itself is generic).
pub trait AccountIdConversion<AccountId>: Sized {
    /// Convert into an account ID. This is infallible.
    fn into_account(&self) -> AccountId;

    /// Try to convert an account ID into this type. Might not succeed.
    fn try_from_account(a: &AccountId) -> Option<Self>;
}

// Code from https://github.com/paritytech/polkadot/blob/rococo-v1/parachain/src/primitives.rs
// This will be moved to own crate and can remove
struct TrailingZeroInput<'a>(&'a [u8]);

impl<'a> codec::Input for TrailingZeroInput<'a> {
    fn remaining_len(&mut self) -> Result<Option<usize>, CodecError> {
        Ok(None)
    }

    fn read(&mut self, into: &mut [u8]) -> Result<(), CodecError> {
        let len = into.len().min(self.0.len());
        into[..len].copy_from_slice(&self.0[..len]);
        for i in &mut into[len..] {
            *i = 0;
        }
        self.0 = &self.0[len..];
        Ok(())
    }
}

// Code modified from https://github.com/paritytech/polkadot/blob/rococo-v1/parachain/src/primitives.rs
/// Format is b"miner" ++ encode(minerId) ++ 00.... where 00... is indefinite trailing
/// zeroes to fill AccountId.
impl<T: Encode + Decode> AccountIdConversion<T> for MinerId {
    fn into_account(&self) -> T {
        (b"miner", self)
            .using_encoded(|b| T::decode(&mut TrailingZeroInput(b)))
            .unwrap()
    }

    fn try_from_account(x: &T) -> Option<Self> {
        x.using_encoded(|d| {
            if &d[0..5] != b"miner" {
                return None;
            }
            let mut cursor = &d[5..];
            let result = Decode::decode(&mut cursor).ok()?;
            if cursor.iter().all(|x| *x == 0) {
                Some(result)
            } else {
                None
            }
        })
    }
}
