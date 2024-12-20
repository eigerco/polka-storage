use core::{fmt::Display, marker::PhantomData};

use codec::{Decode, Encode, MaxEncodedLen};
use scale_decode::{
    visitor::{self},
    DecodeAsType, ToString, TypeResolver, Visitor,
};
use scale_encode::EncodeAsType;
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

use crate::MAX_SECTORS;

/// SectorNumber is a unique identifier for a sector.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Ord,
    PartialOrd,
    Eq,
    Encode,
    EncodeAsType,
    TypeInfo,
    RuntimeDebug,
    MaxEncodedLen,
)]
#[cfg_attr(feature = "serde", derive(::serde::Serialize))]
pub struct SectorNumber(u32);

impl SectorNumber {
    /// Creates a new `SectorNumber` instance.
    ///
    /// Returns a `Result` containing the new `SectorNumber` if valid,
    /// or an error message if the sector number exceeds `MAX_SECTORS`.
    pub fn new(sector_number: u32) -> Result<Self, SectorNumberError> {
        if sector_number > MAX_SECTORS {
            return Err(SectorNumberError::NumberTooLarge);
        }

        Ok(Self(sector_number))
    }
}

#[cfg(feature = "serde")]
impl<'de> ::serde::Deserialize<'de> for SectorNumber {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: ::serde::Deserializer<'de>,
    {
        let value = u32::deserialize(deserializer)?;
        SectorNumber::new(value).map_err(|_| {
            ::serde::de::Error::invalid_value(
                ::serde::de::Unexpected::Unsigned(value as u64),
                &"an integer between 0 and MAX_SECTORS",
            )
        })
    }
}

impl Decode for SectorNumber {
    fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
        let value = u32::decode(input)?;
        SectorNumber::new(value).map_err(|_| "Sector number is too large".into())
    }
}

#[derive(
    Clone,
    Copy,
    PartialEq,
    Ord,
    PartialOrd,
    Eq,
    Encode,
    EncodeAsType,
    TypeInfo,
    RuntimeDebug,
    MaxEncodedLen,
    thiserror::Error,
)]
pub enum SectorNumberError {
    #[error("Sector number is too large")]
    NumberTooLarge,
}

// Implement the `Visitor` trait to define how to go from SCALE
// values into this type.
pub struct SectorNumberVisitor<R>(PhantomData<R>);

impl<R> SectorNumberVisitor<R> {
    fn new() -> Self {
        Self(PhantomData)
    }
}

impl<R: TypeResolver> Visitor for SectorNumberVisitor<R> {
    type Value<'scale, 'resolver> = SectorNumber;
    type Error = scale_decode::Error;
    type TypeResolver = R;

    fn visit_u32<'scale, 'resolver>(
        self,
        value: u32,
        _type_id: visitor::TypeIdFor<Self>,
    ) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
        SectorNumber::new(value).map_err(|_| {
            scale_decode::Error::new(scale_decode::error::ErrorKind::NumberOutOfRange {
                value: value.to_string(),
            })
        })
    }

    fn visit_composite<'scale, 'resolver>(
        self,
        value: &mut visitor::types::Composite<'scale, 'resolver, Self::TypeResolver>,
        _type_id: visitor::TypeIdFor<Self>,
    ) -> Result<Self::Value<'scale, 'resolver>, Self::Error> {
        // `visit_composite` is called when the type is part of some other
        // composite type.
        match value.decode_item(self) {
            Some(item) => item,
            None => {
                return Err(scale_decode::Error::new(
                    scale_decode::error::ErrorKind::CannotFindField {
                        name: "".to_string(),
                    },
                ))
            }
        }
    }
}

impl scale_decode::IntoVisitor for SectorNumber {
    type AnyVisitor<R: TypeResolver> = SectorNumberVisitor<R>;
    fn into_visitor<R: TypeResolver>() -> Self::AnyVisitor<R> {
        SectorNumberVisitor::new()
    }
}

impl From<u16> for SectorNumber {
    fn from(value: u16) -> Self {
        SectorNumber(value as u32)
    }
}

impl TryFrom<u32> for SectorNumber {
    type Error = SectorNumberError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SectorNumber> for u32 {
    fn from(value: SectorNumber) -> Self {
        value.0
    }
}

impl From<SectorNumber> for u64 {
    fn from(value: SectorNumber) -> Self {
        value.0 as u64
    }
}

impl Display for SectorNumber {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(
    Encode, Decode, DecodeAsType, EncodeAsType, TypeInfo, Clone, RuntimeDebug, PartialEq, Eq, Copy,
)]
#[cfg_attr(feature = "serde", derive(::serde::Deserialize, ::serde::Serialize))]
#[codec(crate = ::codec)]
#[decode_as_type(crate_path = "::scale_decode")]
#[encode_as_type(crate_path = "::scale_encode")]
pub enum SectorSize {
    _2KiB,
    _8MiB,
    _512MiB,
    _32GiB,
    _64GiB,
}

impl SectorSize {
    /// Returns the size of a sector in bytes
    /// <https://github.com/filecoin-project/ref-fvm/blob/5659196fa94accdf1e7f10e00586a8166c44a60d/shared/src/sector/mod.rs#L40>
    pub fn bytes(&self) -> u64 {
        match self {
            SectorSize::_2KiB => 2 << 10,
            SectorSize::_8MiB => 8 << 20,
            SectorSize::_512MiB => 512 << 20,
            SectorSize::_32GiB => 32 << 30,
            SectorSize::_64GiB => 2 * (32 << 30),
        }
    }
}

impl core::fmt::Display for SectorSize {
    fn fmt(
        &self,
        f: &mut scale_info::prelude::fmt::Formatter<'_>,
    ) -> scale_info::prelude::fmt::Result {
        match self {
            SectorSize::_2KiB => write!(f, "2KiB"),
            SectorSize::_8MiB => write!(f, "8MiB"),
            SectorSize::_512MiB => write!(f, "512MiB"),
            SectorSize::_32GiB => write!(f, "32GiB"),
            SectorSize::_64GiB => write!(f, "64GiB"),
        }
    }
}
