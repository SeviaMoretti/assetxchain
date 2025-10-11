extern crate alloc;
use alloc::format;
use codec::{Encode, Decode, MaxEncodedLen};
use sp_std::vec::Vec;
use sp_core::{H256, H160};
use scale_info::TypeInfo;

// Protocol version constants
pub const ASSET_PROTOCOL_VERSION: &str = "1.0";
pub const RIGHT_TOKEN_PROTOCOL_VERSION: &str = "1.0";

/// Data Asset Structure
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct DataAsset {
    // Protocol version
    pub version: Vec<u8>,
    
    // Unique asset identifier
    pub asset_id: [u8; 32],
    
    // Sequential token ID assigned by contract
    pub token_id: u32,
    
    // Basic information
    pub name: Vec<u8>,
    pub description: Vec<u8>,
    pub quantity: Vec<u8>,
    pub labels: Vec<Vec<u8>>,
    
    // Data characteristics
    pub statistical_characteristic: Vec<u8>,
    pub analyzing_feature: Vec<u8>,
    pub integrity: Vec<u8>,
    pub raw_data_hash: H256,
    
    // Ownership
    pub owner: H160,
    
    // IPFS storage info
    pub metadata_cid: Vec<u8>,
    pub data_cid_merkle_nodes: Vec<MerkleNode>,
    
    // Timestamps and signature
    pub timestamp: u64,
    pub confirm_time: u64,
    pub signature: Vec<u8>,
    
    // Transaction and state info
    pub nonce: u32,
    pub is_locked: bool,
    
    // Encryption info
    pub encryption_info: EncryptionInfo,
    
    // Certificate sub-tree root hash
    pub children_root: [u8; 32],
    
    // Statistics
    pub view_count: u64,
    pub download_count: u64,
    pub transaction_count: u64,
    pub total_revenue: u128,
    
    // Pricing configuration
    pub pricing_config: PricingConfig,
    
    // Asset status
    pub status: AssetStatus,
    
    // Update timestamp
    pub updated_at: u64,
}

/// Right Token (Certificate) Structure
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct RightToken {
    // Protocol version
    pub version: Vec<u8>,
    
    // Token ID format: parent_token_id|certificate_id
    pub token_id: Vec<u8>,
    
    // Unique certificate identifier
    pub certificate_id: u32,
    
    // Right type
    pub right_type: RightType,
    
    // Time information
    pub create_time: u64,
    pub confirm_time: u64,
    pub valid_from: u64,
    pub valid_until: Option<u64>,
    
    // Ownership
    pub owner: H160,
    pub issuer: H160,
    
    // Transaction info
    pub nonce: u32,
    
    // Parent asset reference
    pub parent_asset_id: [u8; 32],
    pub parent_asset_token_id: u32,
    
    // Certificate status
    pub status: CertificateStatus,
    
    // Traceability
    pub right_token_from: Option<Vec<u8>>,
    
    // Signature
    pub signature: Vec<u8>,
}

/// Encryption Information
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct EncryptionInfo {
    pub algorithm: Vec<u8>,
    pub key_length: u32,
    pub parameters_hash: H256,
    pub is_encrypted: bool,
}

/// Merkle Tree Node
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct MerkleNode {
    pub hash: H256,
    pub is_leaf: bool,
    pub data: Option<Vec<u8>>,
}

/// Right Type Enumeration
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum RightType {
    Usage = 1,
    Access = 2,
}

/// Asset Status Enumeration
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum AssetStatus {
    Active = 1,
    Locked = 2,
}

/// Certificate Status Enumeration
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum CertificateStatus {
    Active = 1,
    Expired = 2,
}

/// Pricing Configuration
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct PricingConfig {
    pub base_price: u128,
    pub currency: Vec<u8>,
}

// Default implementations
impl Default for DataAsset {
    fn default() -> Self {
        Self {
            version: ASSET_PROTOCOL_VERSION.as_bytes().to_vec(),
            asset_id: [0u8; 32],
            token_id: 0,
            name: Vec::new(),
            description: Vec::new(),
            quantity: Vec::new(),
            labels: Vec::new(),
            statistical_characteristic: Vec::new(),
            analyzing_feature: Vec::new(),
            integrity: Vec::new(),
            raw_data_hash: H256::zero(),
            owner: H160::zero(),
            metadata_cid: Vec::new(),
            data_cid_merkle_nodes: Vec::new(),
            timestamp: 0,
            confirm_time: 0,
            signature: Vec::new(),
            nonce: 0,
            is_locked: false,
            encryption_info: EncryptionInfo::default(),
            children_root: [0u8; 32],
            view_count: 0,
            download_count: 0,
            transaction_count: 0,
            total_revenue: 0,
            pricing_config: PricingConfig::default(),
            status: AssetStatus::Active,
            updated_at: 0,
        }
    }
}

impl Default for RightToken {
    fn default() -> Self {
        Self {
            version: RIGHT_TOKEN_PROTOCOL_VERSION.as_bytes().to_vec(),
            token_id: Vec::new(),
            certificate_id: 0,
            right_type: RightType::Usage,
            create_time: 0,
            confirm_time: 0,
            valid_from: 0,
            valid_until: None,
            owner: H160::zero(),
            issuer: H160::zero(),
            nonce: 0,
            parent_asset_id: [0u8; 32],
            parent_asset_token_id: 0,
            status: CertificateStatus::Active,
            right_token_from: None,
            signature: Vec::new(),
        }
    }
}

impl Default for EncryptionInfo {
    fn default() -> Self {
        Self {
            algorithm: Vec::new(),
            key_length: 0,
            parameters_hash: H256::zero(),
            is_encrypted: false,
        }
    }
}

impl Default for PricingConfig {
    fn default() -> Self {
        Self {
            base_price: 0,
            currency: b"NATIVE".to_vec(),
        }
    }
}

// Utility methods
impl DataAsset {
    /// Generate asset ID from owner, timestamp, and data hash
    pub fn generate_asset_id(owner: &H160, timestamp: u64, data_hash: &H256) -> [u8; 32] {
        use sp_io::hashing::blake2_256;
        
        let mut input = Vec::new();
        input.extend_from_slice(owner.as_bytes());
        input.extend_from_slice(&timestamp.to_le_bytes());
        input.extend_from_slice(data_hash.as_bytes());
        
        blake2_256(&input)
    }
    
    /// Check if asset is locked
    pub fn is_locked(&self) -> bool {
        self.is_locked || self.status == AssetStatus::Locked
    }
    
    /// Check if asset is active
    pub fn is_active(&self) -> bool {
        self.status == AssetStatus::Active && !self.is_locked()
    }
}

impl RightToken {
    /// Generate token ID from parent token ID and certificate sequence
    pub fn generate_token_id(parent_token_id: u32, certificate_sequence: u32) -> Vec<u8> {
        let token_str = format!("{}|{}", parent_token_id, certificate_sequence);
        token_str.into_bytes()
    }
    
    /// Check if certificate is valid at current time
    pub fn is_valid(&self, current_time: u64) -> bool {
        self.status == CertificateStatus::Active &&
        current_time >= self.valid_from &&
        self.valid_until.map_or(true, |until| current_time <= until)
    }
    
    /// Check if certificate is expired
    pub fn is_expired(&self, current_time: u64) -> bool {
        self.valid_until.map_or(false, |until| current_time > until)
    }
}