use std::mem::size_of;

use bytes::{BufMut, BytesMut};
use celestia_proto::proof::pb::Proof as RawProof;
use celestia_proto::share::p2p::shwap::Sample as RawSample;
use cid::CidGeneric;
use multihash::Multihash;
use serde::{Deserialize, Serialize};
use tendermint_proto::Protobuf;

use crate::axis::{AxisId, AxisType};
use crate::multihash::{HasCid, HasMultihash};
use crate::nmt::{NamespaceProof, NamespacedHashExt};
use crate::{DataAvailabilityHeader, ExtendedDataSquare};
use crate::{Error, Result, Share};

const SAMPLE_ID_SIZE: usize = SampleId::size();
pub const SAMPLE_ID_MULTIHASH_CODE: u64 = 0x7801;
pub const SAMPLE_ID_CODEC: u64 = 0x7800;

/// Represents particular sample along the axis on specific Data Square
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct SampleId {
    pub axis: AxisId,
    pub index: u16,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SampleType {
    DataSample,
    ParitySample,
}

impl TryFrom<u8> for SampleType {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SampleType::DataSample),
            1 => Ok(SampleType::ParitySample),
            n => Err(Error::InvalidAxis(n.into())), // TODO
        }
    }
}

impl From<SampleType> for u8 {
    fn from(sample_type: SampleType) -> u8 {
        match sample_type {
            SampleType::DataSample => 0,
            SampleType::ParitySample => 1,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(try_from = "RawSample", into = "RawSample")]
pub struct Sample {
    pub sample_id: SampleId,

    pub sample_type: SampleType,
    pub share: Share,
    pub proof: NamespaceProof,
}

impl Sample {
    pub fn new(
        axis_type: AxisType,
        index: usize,
        dah: &DataAvailabilityHeader,
        eds: &ExtendedDataSquare,
        block_height: u64,
    ) -> Result<Self> {
        let square_len = dah.square_len();

        let axis_index = index / square_len;
        let share_index = index % square_len;

        let shares = eds.axis(axis_type, axis_index, square_len);
    }
}

impl Protobuf<RawSample> for Sample {}

impl TryFrom<RawSample> for Sample {
    type Error = Error;

    fn try_from(sample: RawSample) -> Result<Sample, Self::Error> {
        let Some(proof) = sample.sample_proof else {
            return Err(Error::MissingProof);
        };

        let sample_id = SampleId::decode(&sample.sample_id)?;
        let share = Share::from_raw(&sample.sample_share)?;
        let sample_type = u8::try_from(sample.sample_type)
            .map_err(|_| Error::InvalidAxis(sample.sample_type))?
            .try_into()?;

        Ok(Sample {
            sample_id,
            sample_type,
            share,
            proof: proof.try_into()?,
        })
    }
}

impl From<Sample> for RawSample {
    fn from(sample: Sample) -> RawSample {
        let mut sample_id_bytes = BytesMut::new();
        sample.sample_id.encode(&mut sample_id_bytes);
        let sample_proof = RawProof {
            start: sample.proof.start_idx() as i64,
            end: sample.proof.end_idx() as i64,
            nodes: sample.proof.siblings().iter().map(|h| h.to_vec()).collect(),
            leaf_hash: vec![], // this is an inclusion proof
            is_max_namespace_ignored: true,
        };

        RawSample {
            sample_id: sample_id_bytes.to_vec(),
            sample_share: sample.share.to_vec(),
            sample_type: u8::from(sample.sample_type) as i32,
            sample_proof: Some(sample_proof),
        }
    }
}

impl SampleId {
    /// Create new SampleId. Index references sample number from the entire Data Square (is
    /// converted to row/col coordinates internally). Same location can be sampled row or
    /// column-wise, axis_type is used to distinguish that. Axis root hash is calculated from the
    /// DataAvailabilityHeader
    pub fn new(
        axis_type: AxisType,
        index: usize,
        dah: &DataAvailabilityHeader,
        block_height: u64,
    ) -> Result<Self> {
        let square_len = dah.square_len();

        let (axis_index, sample_index) = match axis_type {
            AxisType::Row => (index / square_len, index % square_len),
            AxisType::Col => (index % square_len, index / square_len),
        };

        Ok(SampleId {
            axis: AxisId::new(axis_type, axis_index, dah, block_height)?,
            index: sample_index
                .try_into()
                .map_err(|_| Error::EdsIndexOutOfRange(sample_index))?,
        })
    }

    /// number of bytes needed to represent `SampleId`
    pub const fn size() -> usize {
        AxisId::size() + size_of::<u16>()
    }

    fn encode(&self, bytes: &mut BytesMut) {
        self.axis.encode(bytes);
        bytes.put_u16_le(self.index);
    }

    fn decode(buffer: &[u8]) -> Result<Self> {
        if buffer.len() != SAMPLE_ID_SIZE {
            return Err(Error::InvalidMultihashLength(buffer.len()));
        }

        let (axis_id, index) = buffer.split_at(AxisId::size());
        // RawSampleId len is defined as AxisId::size + u16::size, these are safe
        Ok(Self {
            axis: AxisId::decode(axis_id)?,
            index: u16::from_le_bytes(index.try_into().unwrap()),
        })
    }
}

impl HasMultihash<SAMPLE_ID_SIZE> for SampleId {
    fn multihash(&self) -> Result<Multihash<SAMPLE_ID_SIZE>> {
        let mut bytes = BytesMut::with_capacity(Self::size());

        self.encode(&mut bytes);

        Ok(Multihash::<SAMPLE_ID_SIZE>::wrap(SAMPLE_ID_MULTIHASH_CODE, &bytes[..]).unwrap())
    }
}

impl HasCid<SAMPLE_ID_SIZE> for SampleId {
    fn codec() -> u64 {
        SAMPLE_ID_CODEC
    }
}

impl<const S: usize> TryFrom<CidGeneric<S>> for SampleId {
    type Error = Error;

    fn try_from(cid: CidGeneric<S>) -> Result<Self, Self::Error> {
        let codec = cid.codec();
        if codec != SAMPLE_ID_CODEC {
            return Err(Error::InvalidCidCodec(codec, SAMPLE_ID_CODEC));
        }

        let hash = cid.hash();

        let size = hash.size() as usize;
        if size != SAMPLE_ID_SIZE {
            return Err(Error::InvalidMultihashLength(size));
        }

        let code = hash.code();
        if code != SAMPLE_ID_MULTIHASH_CODE {
            return Err(Error::InvalidMultihashCode(code, SAMPLE_ID_MULTIHASH_CODE));
        }

        SampleId::decode(hash.digest())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consts::appconsts::SHARE_SIZE;
    use crate::nmt::{Namespace, NamespacedHash, NamespacedHashExt, HASH_SIZE, NS_SIZE};

    #[test]
    fn round_trip() {
        let dah = DataAvailabilityHeader {
            row_roots: vec![NamespacedHash::empty_root(); 10],
            column_roots: vec![NamespacedHash::empty_root(); 10],
        };
        let sample_id = SampleId::new(AxisType::Row, 5, &dah, 100).unwrap();
        let cid = sample_id.cid_v1().unwrap();

        let multihash = cid.hash();
        assert_eq!(multihash.code(), SAMPLE_ID_MULTIHASH_CODE);
        assert_eq!(multihash.size(), SAMPLE_ID_SIZE as u8);

        let deserialized_sample_id = SampleId::try_from(cid).unwrap();
        assert_eq!(sample_id, deserialized_sample_id);
    }

    #[test]
    fn index_calculation() {
        let dah = DataAvailabilityHeader {
            row_roots: vec![NamespacedHash::empty_root(); 8],
            column_roots: vec![NamespacedHash::empty_root(); 8],
        };

        SampleId::new(AxisType::Row, 10, &dah, 100).unwrap();
        SampleId::new(AxisType::Row, 63, &dah, 100).unwrap();
        let sample_err = SampleId::new(AxisType::Row, 64, &dah, 100).unwrap_err();
        assert!(matches!(sample_err, Error::EdsIndexOutOfRange(8)));
        let sample_err = SampleId::new(AxisType::Row, 99, &dah, 100).unwrap_err();
        assert!(matches!(sample_err, Error::EdsIndexOutOfRange(12)));
    }

    #[test]
    fn from_buffer() {
        let bytes = [
            0x01, // CIDv1
            0x80, 0xF0, 0x01, // CID codec = 7800
            0x81, 0xF0, 0x01, // multihash code = 7801
            0x2D, // len = SAMPLE_ID_SIZE = 45
            0,    // axis type = Row = 0
            7, 0, // axis index = 7
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF, // hash
            64, 0, 0, 0, 0, 0, 0, 0, // block height = 64
            5, 0, // sample index = 5
        ];

        let cid = CidGeneric::<SAMPLE_ID_SIZE>::read_bytes(bytes.as_ref()).unwrap();
        assert_eq!(cid.codec(), SAMPLE_ID_CODEC);
        let mh = cid.hash();
        assert_eq!(mh.code(), SAMPLE_ID_MULTIHASH_CODE);
        assert_eq!(mh.size(), SAMPLE_ID_SIZE as u8);
        let sample_id = SampleId::try_from(cid).unwrap();
        assert_eq!(sample_id.axis.axis_type, AxisType::Row);
        assert_eq!(sample_id.axis.index, 7);
        assert_eq!(sample_id.axis.hash, [0xFF; 32]);
        assert_eq!(sample_id.axis.block_height, 64);
        assert_eq!(sample_id.index, 5);
    }

    #[test]
    fn multihash_invalid_code() {
        let multihash = Multihash::<SAMPLE_ID_SIZE>::wrap(888, &[0; SAMPLE_ID_SIZE]).unwrap();
        let cid = CidGeneric::<SAMPLE_ID_SIZE>::new_v1(SAMPLE_ID_CODEC, multihash);
        let axis_err = SampleId::try_from(cid).unwrap_err();
        assert!(matches!(
            axis_err,
            Error::InvalidMultihashCode(888, SAMPLE_ID_MULTIHASH_CODE)
        ));
    }

    #[test]
    fn cid_invalid_codec() {
        let multihash =
            Multihash::<SAMPLE_ID_SIZE>::wrap(SAMPLE_ID_MULTIHASH_CODE, &[0; SAMPLE_ID_SIZE])
                .unwrap();
        let cid = CidGeneric::<SAMPLE_ID_SIZE>::new_v1(4321, multihash);
        let axis_err = SampleId::try_from(cid).unwrap_err();
        assert!(matches!(
            axis_err,
            Error::InvalidCidCodec(4321, SAMPLE_ID_CODEC)
        ));
    }

    #[test]
    fn decode_sample_bytes() {
        let bytes = include_bytes!("../test_data/shwap_samples/sample.data");
        let mut msg = Sample::decode(&bytes[..]).unwrap();

        assert_eq!(msg.sample_id.index, 100);
        assert_eq!(msg.sample_id.axis.axis_type, AxisType::Col);
        assert_eq!(msg.sample_id.axis.index, 64);
        assert_eq!(msg.sample_id.axis.hash, [0xEF; HASH_SIZE]);
        assert_eq!(msg.sample_id.axis.block_height, 255);

        let ns = Namespace::new_v0(&[99]).unwrap();
        assert_eq!(msg.share.namespace(), ns);
        let data = [0xCD; SHARE_SIZE - NS_SIZE];
        assert_eq!(msg.share.data(), data);

        // /*
        msg.sample_id.index = 100;
        msg.sample_id.axis.axis_type = AxisType::Col;
        msg.sample_id.axis.index = 64;
        msg.sample_id.axis.hash = [0xEF; HASH_SIZE];
        msg.sample_id.axis.block_height = 255;

        /*
        let mut i = 0;
        //for share in &mut msg.shares {
            let mut share = &mut msg.share;
            let ns = Namespace::new_v0(&[99]).unwrap();

            let data = [0xCD; crate::consts::appconsts::SHARE_SIZE];
            share.data[..].copy_from_slice(&data);
            share.data[..NS_SIZE].copy_from_slice(ns.as_bytes());

            println!("{i} {:?}", share.namespace());
            //i += 1;
        //}

        let mut file = std::fs::File::create("axis.data2").unwrap();
        let bytes = msg.encode_vec().unwrap();
        file.write_all(&bytes).unwrap();
        */
    }
}
