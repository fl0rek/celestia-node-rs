use serde::{Deserialize, Serialize};

use crate::axis::AxisType;
use crate::Share;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtendedDataSquare {
    #[serde(with = "tendermint_proto::serializers::bytes::vec_base64string")]
    pub data_square: Vec<Vec<u8>>,
    pub codec: String,
}

impl ExtendedDataSquare {
    pub fn row(&self, index: usize, square_len: usize) -> Vec<Share> {
        self.data_square[index * square_len..(index + 1) * square_len]
            .iter()
            .map(|s| Share::from_raw(s))
            .collect::<Result<_, _>>()
            .unwrap()
    }

    pub fn column(&self, mut index: usize, square_len: usize) -> Vec<Share> {
        let mut r = Vec::with_capacity(square_len);
        while index < self.data_square.len() {
            r.push(Share::from_raw(&self.data_square[index]).unwrap());
            index += square_len;
        }
        r
    }

    pub fn axis(&self, axis: AxisType, index: usize, square_len: usize) -> Vec<Share> {
        match axis {
            AxisType::Col => self.column(index, square_len),
            AxisType::Row => self.row(index, square_len),
        }
    }
}
