use cid::CidGeneric;
use multihash::Multihash;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CidError {
    #[error("Invalid multihash lenght")]
    InvalidMultihashLength(usize),

    #[error("Invalid multihash code {0} expected {1}")]
    InvalidMultihashCode(u64, u64),

    #[error("Invalid CID codec {0} expected{1}")]
    InvalidCidCodec(u64, u64),

    #[error("Invalid data format: {0}")]
    InvalidDataFormat(String)
}

pub type Result<T> = std::result::Result<T, CidError>;

pub trait HasMultihash<const S: usize> {
    fn multihash(&self) -> Result<Multihash<S>>;
}

pub trait HasCid<const S: usize>: HasMultihash<S> {
    fn cid_v1(&self) -> Result<CidGeneric<S>> {
        Ok(CidGeneric::<S>::new_v1(Self::codec(), self.multihash()?))
    }

    fn codec() -> u64;
}
