use std::io::Cursor;
use std::result::Result as StdResult;

use bytes::{Buf, BufMut, BytesMut};
use cid::CidGeneric;
use multihash::Multihash;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use sha2::{Digest, Sha256};

use crate::multihash::{HasCid, HasMultihash};
use crate::nmt::{NamespacedHashExt, HASH_SIZE};
use crate::DataAvailabilityHeader;
use crate::{Error, Result};

const AXIS_ID_SIZE: usize = AxisId::size();
pub const AXIS_ID_MULTIHASH_CODE: u64 = 0x7811;
pub const AXIS_ID_CODEC: u64 = 0x7810;

pub type RawAxisId = [u8; AXIS_ID_SIZE];

#[derive(Copy, Clone, Debug, PartialEq, Eq, FromPrimitive)]
#[repr(u8)]
pub enum AxisType {
    Row = 0,
    Col,
}

impl TryFrom<i32> for AxisType {
    type Error = Error;

    fn try_from(value: i32) -> StdResult<Self, Self::Error> {
        FromPrimitive::from_i32(value).ok_or(Error::InvalidAxis(value))
    }
}

#[derive(Debug, PartialEq)]
pub struct AxisId {
    pub axis_type: AxisType,
    pub index: u16,
    pub hash: [u8; HASH_SIZE],
    pub block_height: u64,
}

impl AxisId {
    pub fn new(
        axis_type: AxisType,
        index: usize,
        dah: &DataAvailabilityHeader,
        block_height: u64,
    ) -> Result<Self> {
        if block_height == 0 {
            return Err(Error::ZeroBlockHeight);
        }

        let Some(dah_root) = (match axis_type {
            AxisType::Row => dah.row_root(index),
            AxisType::Col => dah.column_root(index),
        }) else {
            return Err(Error::EdsIndexOutOfRange(index));
        };
        let hash = Sha256::digest(dah_root.to_array()).into();

        Ok(Self {
            axis_type,
            index: index
                .try_into()
                .map_err(|_| Error::EdsIndexOutOfRange(index))?,
            hash,
            block_height,
        })
    }

    pub const fn size() -> usize {
        // size of:
        // u8 + u16 + [u8; 32] + u64
        //  1 +  2  +    32    +  8
        43
    }

    pub fn to_bytes(&self) -> RawAxisId {
        let mut bytes = BytesMut::with_capacity(AXIS_ID_SIZE);
        bytes.put_u8(self.axis_type as u8);
        bytes.put_u16_le(self.index);
        bytes.put(&self.hash[..]);
        bytes.put_u64_le(self.block_height);

        bytes.as_ref().try_into().unwrap()
    }

    pub fn from_bytes(buffer: &RawAxisId) -> Result<Self> {
        let mut cursor = Cursor::new(buffer);

        let axis_type = i32::from(cursor.get_u8()).try_into()?;
        let index = cursor.get_u16_le();
        let hash = cursor.copy_to_bytes(HASH_SIZE).as_ref().try_into().unwrap();
        let block_height = cursor.get_u64_le();

        if block_height == 0 {
            return Err(Error::ZeroBlockHeight);
        }

        Ok(Self {
            axis_type,
            index,
            hash,
            block_height,
        })
    }
}

impl HasMultihash<AXIS_ID_SIZE> for AxisId {
    fn multihash(&self) -> Result<Multihash<AXIS_ID_SIZE>> {
        let digest_bytes = self.to_bytes();
        // length is correct, so unwrap is safe
        Ok(Multihash::wrap(AXIS_ID_MULTIHASH_CODE, &digest_bytes)?)
    }
}

impl HasCid<AXIS_ID_SIZE> for AxisId {
    fn codec() -> u64 {
        AXIS_ID_CODEC
    }
}

impl<const S: usize> TryFrom<CidGeneric<S>> for AxisId {
    type Error = Error;

    fn try_from(cid: CidGeneric<S>) -> Result<Self, Self::Error> {
        let codec = cid.codec();
        if codec != AXIS_ID_CODEC {
            return Err(Error::InvalidCidCodec(codec, AXIS_ID_CODEC));
        }

        let hash = cid.hash();

        let size = hash.size();
        if size as usize != AXIS_ID_SIZE {
            return Err(Error::InvalidMultihashLength(size));
        }

        let code = hash.code();
        if code != AXIS_ID_MULTIHASH_CODE {
            return Err(Error::InvalidMultihashCode(code, AXIS_ID_MULTIHASH_CODE));
        }

        AxisId::from_bytes(hash.digest()[..AXIS_ID_SIZE].try_into().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nmt::NamespacedHash;

    #[test]
    fn axis_type_serialization() {
        assert_eq!(AxisType::Row as u8, 0);
        assert_eq!(AxisType::Col as u8, 1);
    }

    #[test]
    fn axis_type_deserialization() {
        assert_eq!(AxisType::try_from(0).unwrap(), AxisType::Row);
        assert_eq!(AxisType::try_from(1).unwrap(), AxisType::Col);

        let axis_type_err = AxisType::try_from(2).unwrap_err();
        assert!(matches!(axis_type_err, Error::InvalidAxis(2)));
        let axis_type_err = AxisType::try_from(99).unwrap_err();
        assert!(matches!(axis_type_err, Error::InvalidAxis(99)));
    }

    #[test]
    fn round_trip_test() {
        let dah = DataAvailabilityHeader {
            row_roots: vec![NamespacedHash::empty_root(); 10],
            column_roots: vec![NamespacedHash::empty_root(); 10],
        };
        let axis_id = AxisId::new(AxisType::Row, 5, &dah, 100).unwrap();
        let cid = axis_id.cid_v1().unwrap();

        let multihash = cid.hash();
        assert_eq!(multihash.code(), AXIS_ID_MULTIHASH_CODE);
        assert_eq!(multihash.size(), AXIS_ID_SIZE as u8);

        let deserialized_axis_id = AxisId::try_from(cid).unwrap();
        assert_eq!(axis_id, deserialized_axis_id);
    }

    #[test]
    fn index_calculation() {
        let dah = DataAvailabilityHeader {
            row_roots: vec![NamespacedHash::empty_root(); 8],
            column_roots: vec![NamespacedHash::empty_root(); 8],
        };

        AxisId::new(AxisType::Row, 1, &dah, 100).unwrap();
        AxisId::new(AxisType::Row, 7, &dah, 100).unwrap();
        let axis_err = AxisId::new(AxisType::Row, 8, &dah, 100).unwrap_err();
        assert!(matches!(axis_err, Error::EdsIndexOutOfRange(8)));
        let axis_err = AxisId::new(AxisType::Row, 100, &dah, 100).unwrap_err();
        assert!(matches!(axis_err, Error::EdsIndexOutOfRange(100)));
    }

    #[test]
    fn from_buffer() {
        let bytes = [
            0x01, // CIDv1
            0x90, 0xF0, 0x01, // CID codec = 7810
            0x91, 0xF0, 0x01, // multihash code = 7811
            0x2B, // len = AXIS_ID_SIZE = 43
            0,    // axis type = Row = 0
            7, 0, // axis index = 7
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, // hash
            64, 0, 0, 0, 0, 0, 0, 0, // block height = 64
        ];

        let cid = CidGeneric::<AXIS_ID_SIZE>::read_bytes(bytes.as_ref()).unwrap();
        assert_eq!(cid.codec(), AXIS_ID_CODEC);
        let mh = cid.hash();
        assert_eq!(mh.code(), AXIS_ID_MULTIHASH_CODE);
        assert_eq!(mh.size(), AXIS_ID_SIZE as u8);
        let axis_id = AxisId::try_from(cid).unwrap();
        assert_eq!(axis_id.axis_type, AxisType::Row);
        assert_eq!(axis_id.index, 7);
        assert_eq!(axis_id.hash, [0xFF; 32]);
        assert_eq!(axis_id.block_height, 64);
    }

    #[test]
    fn invalid_axis() {
        let bytes = [
            0x01, // CIDv1
            0x90, 0xF0, 0x01, // CID codec = 7810
            0x91, 0xF0, 0x01, // code = 7811
            0x2B, // len = AXIS_ID_SIZE = 43
            0xBE, // invalid axis type!
            7, 0, // axis index = 7
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, // hash
            64, 0, 0, 0, 0, 0, 0, 0, // block height = 64
        ];

        let cid = CidGeneric::<AXIS_ID_SIZE>::read_bytes(bytes.as_ref()).unwrap();
        assert_eq!(cid.codec(), AXIS_ID_CODEC);
        let mh = cid.hash();
        assert_eq!(mh.code(), AXIS_ID_MULTIHASH_CODE);
        assert_eq!(mh.size(), AXIS_ID_SIZE as u8);
        let axis_err = AxisId::try_from(cid).unwrap_err();
        assert!(matches!(axis_err, Error::InvalidAxis(0xBE)));
    }

    #[test]
    fn zero_block_height() {
        let bytes = [
            0x01, // CIDv1
            0x90, 0xF0, 0x01, // CID codec = 7810
            0x91, 0xF0, 0x01, // code = 7811
            0x2B, // len = AXIS_ID_SIZE = 43
            0,    // axis type = Row = 0
            7, 0, // axis index = 7
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, // hash
            0, 0, 0, 0, 0, 0, 0, 0, // invalid block height = 0 !
        ];

        let cid = CidGeneric::<AXIS_ID_SIZE>::read_bytes(bytes.as_ref()).unwrap();
        assert_eq!(cid.codec(), AXIS_ID_CODEC);
        let mh = cid.hash();
        assert_eq!(mh.code(), AXIS_ID_MULTIHASH_CODE);
        assert_eq!(mh.size(), AXIS_ID_SIZE as u8);
        let axis_err = AxisId::try_from(cid).unwrap_err();
        assert!(matches!(axis_err, Error::ZeroBlockHeight));
    }

    #[test]
    fn multihash_invalid_code() {
        let multihash = Multihash::<AXIS_ID_SIZE>::wrap(999, &[0; AXIS_ID_SIZE]).unwrap();
        let cid = CidGeneric::<AXIS_ID_SIZE>::new_v1(AXIS_ID_CODEC, multihash);
        let axis_err = AxisId::try_from(cid).unwrap_err();
        assert!(matches!(
            axis_err,
            Error::InvalidMultihashCode(999, AXIS_ID_MULTIHASH_CODE)
        ));
    }

    #[test]
    fn cid_invalid_codec() {
        let multihash =
            Multihash::<AXIS_ID_SIZE>::wrap(AXIS_ID_MULTIHASH_CODE, &[0; AXIS_ID_SIZE]).unwrap();
        let cid = CidGeneric::<AXIS_ID_SIZE>::new_v1(1234, multihash);
        let axis_err = AxisId::try_from(cid).unwrap_err();
        assert!(matches!(
            axis_err,
            Error::InvalidCidCodec(1234, AXIS_ID_CODEC)
        ));
    }
}
