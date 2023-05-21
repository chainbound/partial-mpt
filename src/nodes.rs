use std::{collections::HashMap, fmt};

use bytes::BytesMut;
use ethers::{
    prelude::EthDisplay,
    types::{Bytes, H256},
    utils::{
        hex, keccak256,
        rlp::{self, Rlp},
    },
};

use crate::{nibbles::Nibbles, Error};

#[derive(Clone, Debug, EthDisplay, PartialEq)]
pub struct Nodes(HashMap<H256, NodeData>);

impl Nodes {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn get(&self, hash: &H256) -> Option<&NodeData> {
        self.0.get(hash)
    }

    #[allow(dead_code)]
    pub fn get_str(&self, hash_str: &str) -> Option<&NodeData> {
        let hash = hash_str.parse::<H256>().unwrap();
        self.get(&hash)
    }

    pub fn insert(&mut self, node_data: NodeData) -> Result<(H256, Option<NodeData>), Error> {
        let key = node_data.hash()?;
        Ok((key, self.0.insert(key, node_data)))
    }

    pub fn remove(&mut self, hash: &H256) -> Option<NodeData> {
        self.0.remove(hash)
    }

    pub fn create_leaf(&mut self, key: Nibbles, value: Bytes) -> Result<H256, Error> {
        let (hash_leaf, _) = self.insert(NodeData::Leaf { key, value })?;
        Ok(hash_leaf)
    }

    pub fn create_branch_or_extension(
        &mut self,
        key_a: Nibbles,
        value_a: Bytes,
        key_b: Nibbles,
        value_b: Bytes,
    ) -> Result<NodeData, Error> {
        let mut branch_node_arr: [Option<H256>; 17] = [None; 17];

        let intersection = key_a.intersect(&key_b)?;

        let key_a_prime = key_a.slice(intersection.len())?;
        let key_b_prime = key_b.slice(intersection.len())?;

        let nibble_a = key_a_prime.first_nibble() as usize;
        let nibble_b = key_b_prime.first_nibble() as usize;

        let hash_a = self.create_leaf(key_a_prime, value_a)?;
        let hash_b = self.create_leaf(key_b_prime, value_b)?;

        branch_node_arr[nibble_a] = Some(hash_a);
        branch_node_arr[nibble_b] = Some(hash_b);

        let branch = NodeData::Branch(branch_node_arr);

        if intersection.len() > 0 {
            let (branch_hash, _) = self.insert(branch)?;

            Ok(NodeData::Extension {
                key: intersection,
                node: branch_hash,
            })
        } else {
            Ok(branch)
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum NodeData {
    // Unknown,
    Leaf { key: Nibbles, value: Bytes },
    Branch([Option<H256>; 17]),
    Extension { key: Nibbles, node: H256 },
}

impl NodeData {
    pub fn hash(&self) -> Result<H256, Error> {
        Ok(H256::from(keccak256(self.to_raw_rlp()?)))
    }

    #[allow(dead_code)]
    pub fn is_leaf(&self) -> bool {
        match self {
            NodeData::Leaf { .. } => true,
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub fn is_branch(&self) -> bool {
        match self {
            NodeData::Branch(_) => true,
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub fn is_extension(&self) -> bool {
        match self {
            NodeData::Extension { .. } => true,
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub fn set_value_on_leaf(&mut self, new_value: Bytes) -> Result<(), Error> {
        match self {
            NodeData::Leaf { key: _, value } => {
                *value = new_value.clone();
                Ok(())
            }
            _ => Err(Error::InternalError(
                "set_value_on_leaf is only valid on leaf nodes",
            )),
        }
    }

    pub fn from_raw_rlp(raw: Bytes) -> Result<Self, Error> {
        let rlp = Rlp::new(&raw);
        let num_items = rlp.item_count()?;
        match num_items {
            2 => Ok({
                let val_0 = Bytes::from(rlp.at(0)?.data()?.to_owned());
                let val_1 = Bytes::from(rlp.at(1)?.data()?.to_owned());

                let (key, terminator) = Nibbles::from_encoded_path_with_terminator(val_0.clone())?;
                if terminator {
                    NodeData::Leaf { key, value: val_1 }
                } else {
                    let hash = rlp.at(1)?.data()?.to_owned();
                    if hash.len() != 32 {
                        return Err(Error::InternalError("invalid hash length in Extension"));
                    }
                    NodeData::Extension {
                        key,
                        node: H256::from_slice(hash.as_slice()),
                    }
                }
            }),
            17 => Ok({
                let mut arr: [Option<H256>; 17] = Default::default();
                for i in 0..17 {
                    let value = rlp.at(i)?.data()?.to_owned();
                    arr[i] = match value.len() {
                        32 => Ok(Some(H256::from_slice(value.as_slice()))),
                        0 => Ok(None),
                        _ => Err(Error::InternalError("invalid hash length in Extension")),
                    }?
                }
                NodeData::Branch(arr)
            }),
            _ => Err(Error::InternalError("Unknown num_items")),
        }
    }

    pub fn to_raw_rlp(&self) -> Result<Bytes, Error> {
        let mut rlp_stream = rlp::RlpStream::new();
        match self {
            NodeData::Leaf { key, value } => {
                let key_bm = BytesMut::from(key.encode_path(true).to_vec().as_slice());
                let value_bm = BytesMut::from(value.to_vec().as_slice());
                rlp_stream.begin_list(2);
                rlp_stream.append(&key_bm);
                rlp_stream.append(&value_bm);
            }
            NodeData::Branch(arr) => {
                rlp_stream.begin_list(17);
                for entry in arr.iter() {
                    let bm = if entry.is_some() {
                        BytesMut::from(entry.to_owned().unwrap().as_bytes())
                    } else {
                        BytesMut::new()
                    };
                    rlp_stream.append(&bm);
                }
            }
            NodeData::Extension { key, node } => {
                let key_bm = BytesMut::from(key.encode_path(false).to_vec().as_slice());
                let value_bm = BytesMut::from(node.as_bytes());
                rlp_stream.begin_list(2);
                rlp_stream.append(&key_bm);
                rlp_stream.append(&value_bm);
            }
        }
        Ok(Bytes::from(rlp_stream.out().to_vec()))
    }
}

impl fmt::Debug for NodeData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let val = match self {
            // NodeData::Unknown => format!("Unknown"),
            NodeData::Leaf { key, value } => format!(
                "Leaf(key={}, value={:?})",
                key,
                hex::encode(value.to_owned())
            ),
            NodeData::Branch(branch) => format!(
                "Branch({:?}",
                branch
                    .iter()
                    .map(|node| {
                        if let Some(node) = node {
                            format!("{:?}", node)
                        } else {
                            format!("None")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            NodeData::Extension { key, node } => {
                format!("Extension(key={}, node={:?})", key, node)
            }
        };
        write!(f, "NodeData::{}", val)
    }
}

#[cfg(test)]
mod tests {
    use super::{Nibbles, NodeData};
    use ethers::utils::hex;

    #[test]
    pub fn test_node_data_new_leaf_node_1() {
        let node_data = NodeData::from_raw_rlp(
            "0xe3a120290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e56308"
                .parse()
                .unwrap(),
        )
        .unwrap();

        println!("node_data {:#?}", node_data);

        assert_eq!(
            node_data,
            NodeData::Leaf {
                key: Nibbles::from_raw_path(
                    "0x290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e563"
                        .parse()
                        .unwrap()
                ),
                value: "0x08".parse().unwrap(),
            }
        );
    }

    #[test]
    pub fn test_node_data_new_leaf_node_2() {
        let input_raw_rlp =
            "e3a120290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e56308";
        let node_data = NodeData::from_raw_rlp(input_raw_rlp.parse().unwrap()).unwrap();
        assert_eq!(hex::encode(node_data.to_raw_rlp().unwrap()), input_raw_rlp);
    }

    #[test]
    pub fn test_node_data_new_leaf_node_3() {
        let input_raw_rlp =
            "e3a120290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e56308";
        let mut node_data = NodeData::from_raw_rlp(input_raw_rlp.parse().unwrap()).unwrap();
        node_data
            .set_value_on_leaf("0x01".parse().unwrap())
            .unwrap();
        assert_eq!(
            hex::encode(node_data.to_raw_rlp().unwrap()),
            "e3a120290decd9548b62a8d60345a988386fc84ba6bc95484008f6362f93160ef3e56301" // 8 changed to 1
        );
    }

    #[test]
    pub fn test_node_data_new_extension_node_1() {
        let node_data = NodeData::from_raw_rlp(
            "0xe583165a7ba0e46db0426b9d34c7b2df7baf0480777946e6b5b74a0572592b0229a4edaed944"
                .parse()
                .unwrap(),
        )
        .unwrap();

        println!("node_data {:#?}", node_data);

        assert_eq!(
            node_data,
            NodeData::Extension {
                key: Nibbles::from_encoded_path("0x165a7b".parse().unwrap()).unwrap(),
                node: "0xe46db0426b9d34c7b2df7baf0480777946e6b5b74a0572592b0229a4edaed944"
                    .parse()
                    .unwrap(),
            }
        );
    }

    #[test]
    pub fn test_node_data_new_extension_node_2() {
        let input_raw_rlp =
            "e583165a7ba0e46db0426b9d34c7b2df7baf0480777946e6b5b74a0572592b0229a4edaed944";
        let node_data = NodeData::from_raw_rlp(input_raw_rlp.parse().unwrap()).unwrap();
        assert_eq!(hex::encode(node_data.to_raw_rlp().unwrap()), input_raw_rlp);
    }

    #[test]
    pub fn test_node_data_new_branch_1() {
        let node_data = NodeData::from_raw_rlp(
            "0xf851a0e97150c3ed221a6f46bdcd44e8a2d44825bc781fa48f797e9df2f8ceff52a43e8080808080808080808080a09487c8e7f28469b9f72cd6be094b555c3882c0653f11b208ff76bf8caee5043280808080"
                .parse()
                .unwrap(),
        )
        .unwrap();

        println!("node_data {:#?}", node_data);

        assert_eq!(
            node_data,
            NodeData::Branch([
                Some(
                    "0xe97150c3ed221a6f46bdcd44e8a2d44825bc781fa48f797e9df2f8ceff52a43e"
                        .parse()
                        .unwrap()
                ),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                Some(
                    "0x9487c8e7f28469b9f72cd6be094b555c3882c0653f11b208ff76bf8caee50432"
                        .parse()
                        .unwrap()
                ),
                None,
                None,
                None,
                None,
            ])
        );
    }

    #[test]
    pub fn test_node_data_new_branch_2() {
        let input_raw_rlp =
            "f851a0e97150c3ed221a6f46bdcd44e8a2d44825bc781fa48f797e9df2f8ceff52a43e8080808080808080808080a09487c8e7f28469b9f72cd6be094b555c3882c0653f11b208ff76bf8caee5043280808080";
        let node_data = NodeData::from_raw_rlp(input_raw_rlp.parse().unwrap()).unwrap();
        assert_eq!(hex::encode(node_data.to_raw_rlp().unwrap()), input_raw_rlp);
    }
}