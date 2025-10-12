extern crate alloc;
use alloc::format;
use codec::{Encode, Decode};
use sp_std::vec::Vec;
use sp_core::{H256, H160};
use scale_info::TypeInfo;

// Protocol version constants
pub const ASSET_PROTOCOL_VERSION: &str = "1.0";
pub const RIGHT_TOKEN_PROTOCOL_VERSION: &str = "1.0";

/// Data Asset Structure
// ！！！！结构体字段太多了，要拆分成几个子结构体1、核心dataasset2、assetMetadata3、统计数据4、加密信息等
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct DataAsset<AccountId> {
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
    pub owner: AccountId,
    
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
pub struct RightToken<AccountId> {
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
    pub owner: AccountId,
    pub issuer: AccountId,
    
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
impl<AccountId: Default> Default for DataAsset<AccountId> {
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
            owner: AccountId::default(),
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

impl<AccountId: Default> Default for RightToken<AccountId> {
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
            owner: AccountId::default(),
            issuer: AccountId::default(),
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
impl<AccountId: Clone> DataAsset<AccountId> {
    /// Generate asset ID from owner, timestamp, and data hash
    pub fn generate_asset_id(owner: &AccountId, timestamp: u64, data_hash: &H256) -> [u8; 32]
    where
        AccountId: Encode,
    {
        use sp_io::hashing::blake2_256;
        
        let mut input = Vec::new();
        input.extend_from_slice(&owner.encode());
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

impl<AccountId> RightToken<AccountId> {
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

// Builder pattern constructors
impl<AccountId: Clone + Encode> DataAsset<AccountId> {
    /// Create a minimal DataAsset with only required fields
    pub fn minimal(
        owner: AccountId, 
        name: Vec<u8>, 
        description: Vec<u8>, 
        raw_data_hash: H256, 
        timestamp: u64
    ) -> Self {
        Self {
            // Protocol version
            version: b"1.0".to_vec(),
            
            // IDs (will be set by caller)
            asset_id: [0u8; 32],
            token_id: 0,
            
            // Basic info
            name,
            description,
            quantity: Vec::new(),
            labels: Vec::new(),
            
            // Data characteristics
            statistical_characteristic: Vec::new(),
            analyzing_feature: Vec::new(),
            integrity: Vec::new(),
            raw_data_hash,
            
            // Ownership
            owner,
            
            // IPFS storage
            metadata_cid: Vec::new(),
            data_cid_merkle_nodes: Vec::new(),
            
            // Timestamps
            timestamp,
            confirm_time: timestamp,
            signature: Vec::new(),
            
            // Transaction state
            nonce: 0,
            is_locked: false,
            
            // Encryption
            encryption_info: EncryptionInfo {
                algorithm: Vec::new(),
                key_length: 0,
                parameters_hash: H256::zero(),
                is_encrypted: false,
            },
            
            // Certificate root
            children_root: [0u8; 32],
            
            // Statistics
            view_count: 0,
            download_count: 0,
            transaction_count: 0,
            total_revenue: 0,
            
            // Pricing
            pricing_config: PricingConfig {
                base_price: 0,
                currency: b"NATIVE".to_vec(),
            },
            
            // Status
            status: AssetStatus::Active,
            updated_at: timestamp,
        }
    }
}

impl<AccountId: Clone> RightToken<AccountId> {
    /// Create a minimal RightToken with only required fields
    pub fn minimal(
        certificate_id: u32,
        right_type: RightType,
        holder: AccountId,
        issuer: AccountId,
        parent_asset_id: [u8; 32],
        parent_asset_token_id: u32,
        current_time: u64,
        valid_until: Option<u64>
    ) -> Self {
        let token_id = Self::generate_token_id(parent_asset_token_id, certificate_id);
        
        Self {
            // Protocol version
            version: b"1.0".to_vec(),
            
            // Token ID (will be set by caller)
            token_id,
            
            // Certificate ID
            certificate_id,
            
            // Right type
            right_type,
            
            // Time info
            create_time: current_time,
            confirm_time: current_time,
            valid_from: current_time,
            valid_until,
            
            // Ownership
            owner: holder,
            issuer,
            
            // Transaction info
            nonce: 0,
            
            // Parent asset reference
            parent_asset_id,
            parent_asset_token_id,
            
            // Status
            status: CertificateStatus::Active,
            
            // Traceability
            right_token_from: None,
            
            // Signature
            signature: Vec::new(),
        }
    }
}