use codec::{Encode, Decode};
use sp_core::H256;
use sp_runtime::DigestItem;
use alloc::vec::Vec; 

const ASSET_ROOT_PREFIX: &[u8] = b"ASSET_ROOT";

pub fn create_asset_root_digest(root: H256) -> DigestItem {
    let mut data = Vec::new();
    data.extend_from_slice(ASSET_ROOT_PREFIX);
    data.extend_from_slice(&root.encode());
    DigestItem::Other(data)
}

pub fn extract_asset_root(digest: &sp_runtime::Digest) -> Option<H256> {
    for log in digest.logs.iter() {
        if let DigestItem::Other(data) = log {
            if data.len() > ASSET_ROOT_PREFIX.len() 
                && &data[..ASSET_ROOT_PREFIX.len()] == ASSET_ROOT_PREFIX 
            {
                if let Ok(root) = H256::decode(&mut &data[ASSET_ROOT_PREFIX.len()..]) {
                    return Some(root);
                }
            }
        }
    }
    None
}