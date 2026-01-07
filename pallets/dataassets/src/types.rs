extern crate alloc;
use codec::{Encode, Decode, MaxEncodedLen};
use sp_std::vec::Vec;
use sp_core::{H256};
use scale_info::TypeInfo;
use frame_support::{BoundedVec, traits::Get, traits::ConstU32};

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
    // pub data_cid_merkle_nodes: Vec<MerkleNode>, // 之后用
    
    // Timestamps and signature
    pub timestamp: u64,
    pub signature: Vec<u8>,
    
    // Transaction and state info
    pub nonce: u32,
    pub is_locked: bool, // ！！！！！！！！！和status重复了
    
    // Encryption info
    pub encryption_info: EncryptionInfo,
    
    // Certificate sub-tree root hash
    pub children_root: [u8; 32],
    
    // Statistics
    pub view_count: u64,
    pub transaction_count: u64, // ！！！！！！！！多余了，已经有nonce了
    pub total_revenue: u128, // 总收益，权证销售额
    
    // Pricing configuration
    pub pricing_config: PricingConfig,
    
    // Asset status
    pub status: AssetStatus,
    
    // Update timestamp
    pub updated_at: u64,
    // 活力值，用来评估资产的使用价值和活跃度，受权证销售情况影响
    // 生命值，与IPFS存储池和加密后的数据大小相关
    // 
}

/// Right Token (Certificate) Structure
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct RightToken<AccountId> {
    // Protocol version
    pub version: Vec<u8>,
    
    // Unique certificate identifier
    pub certificate_id: [u8; 32],

    // Token ID format: parent_token_id|certificate_id
    pub token_id: u32,
    
    // Right type
    pub right_type: RightType,
    
    // Time information
    pub create_time: u64,
    // 权证是一次性的，过了限制条件就没了
    pub valid_from: u64, // 生效时间
    pub valid_until: Option<u64>, // 过期时间，None表示永不过期
    
    // Ownership
    pub owner: AccountId,
    pub issuer: AccountId,
    
    // Transaction info
    pub nonce: u32,
    
    // Parent asset reference
    pub parent_asset_id: [u8; 32],
    
    // Certificate status
    pub status: CertificateStatus,
    
    // Signature
    pub signature: Vec<u8>,
}

/// Collateral Information for Asset
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct CollateralInfo<AccountId, Balance, BlockNumber> {
    /// The account that deposited the collateral
    pub depositor: AccountId,
    
    /// Total collateral amount required
    pub total_amount: Balance,
    
    /// Amount still reserved/locked
    pub reserved_amount: Balance,
    
    /// Amount that has been released
    pub released_amount: Balance,
    
    /// Release schedule with phases
    pub release_schedule: BoundedVec<ReleasePhase<BlockNumber, Balance>, ConstU32<5>>,
    
    /// Current status of the collateral
    pub status: CollateralStatus<Balance>,
}

/// Release Phase for Collateral
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub struct ReleasePhase<BlockNumber, Balance> {
    /// Percentage of total collateral (50%, 30%, 20%)
    pub percentage: u8,
    
    /// Actual amount to be released in this phase
    pub amount: Balance,
    
    /// Block number when this phase can be unlocked
    pub unlock_block: BlockNumber,
    
    /// Condition that must be met for release
    pub condition: ReleaseCondition,
    
    /// Whether this phase has been released
    pub is_released: bool,
}

/// Conditions for Releasing Collateral
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum ReleaseCondition {
    /// Only time requirement (no additional conditions)
    TimeOnly,
    
    /// Time + data verification passed
    TimeAndVerification,
    
    /// Time + at least one certificate usage
    TimeAndUsage,
    
    /// Time + IPFS data continuously available
    TimeAndAvailability,
}

/// Collateral Status
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo, MaxEncodedLen)]
pub enum CollateralStatus<Balance> {
    /// All collateral is locked
    FullyLocked,
    
    /// Some collateral has been released
    PartiallyReleased,
    
    /// All collateral has been released
    FullyReleased,
    
    /// Collateral was slashed (contains slashed amount)
    Slashed(Balance),
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
    Private = 1, // 私有资产，只有资产所有者可以使用
    Locked = 2,
    Approved = 3, // 已授权，被资产所有者授权给市场 ！！！将这个删除，要判断资产是否已授权的话，
    // 直接判断 AssetApprovals::<T>::contains_key(&asset_id) 是否为 true 即可，每次授权都修改资产状态不划算
}

/// Certificate Status Enumeration
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum CertificateStatus {
    Active = 1,
    Expired = 2,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum PriceType {
    Fixed, // 固定价格
    Negotiable, // 协商价格
}

/// Pricing Configuration
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct PricingConfig {
    pub price_type: PriceType,
    pub currency: Vec<u8>,

    pub base_price: u128, // 元证价格
    pub usage_price: u128, // 使用权价格（权证）
    pub access_price: u128, // 访问权价格（权证）
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
            // data_cid_merkle_nodes: Vec::new(),
            timestamp: 0,
            signature: Vec::new(),
            nonce: 0,
            is_locked: false,
            encryption_info: EncryptionInfo::default(),
            children_root: [0u8; 32],
            view_count: 0,
            transaction_count: 0,
            total_revenue: 0,
            pricing_config: PricingConfig::default(),
            status: AssetStatus::Private,
            updated_at: 0,
        }
    }
}

impl<AccountId: Default> Default for RightToken<AccountId> {
    fn default() -> Self {
        Self {
            version: RIGHT_TOKEN_PROTOCOL_VERSION.as_bytes().to_vec(),
            token_id: 0,
            certificate_id: [0u8; 32],
            right_type: RightType::Usage,
            create_time: 0,
            valid_from: 0,
            valid_until: None,
            owner: AccountId::default(),
            issuer: AccountId::default(),
            nonce: 0,
            parent_asset_id: [0u8; 32],
            status: CertificateStatus::Active,
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
            price_type: PriceType::Fixed,
            currency: b"NATIVE".to_vec(),
            base_price: 0,
            usage_price: 0,
            access_price: 0,
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

    pub fn is_approved(&self) -> bool {
        self.status == AssetStatus::Approved
    }
    
    /// Check if asset is active
    /// 应该修改成is_not_locked
    pub fn is_active(&self) -> bool {
        self.status == AssetStatus::Private && !self.is_locked()
    }
}

impl<AccountId: Clone> RightToken<AccountId> {
    /// Generate unique certificate ID
    pub fn generate_certificate_id(parent_asset_id: &[u8; 32], timestamp: u64, issuer: &AccountId) -> [u8; 32] 
    where 
        AccountId: Encode,
    {
        use sp_io::hashing::blake2_256;
        
        let mut input = Vec::new();
        input.extend_from_slice(parent_asset_id);
        input.extend_from_slice(&timestamp.to_le_bytes());
        input.extend_from_slice(&issuer.encode());
        
        blake2_256(&input)
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
            // data_cid_merkle_nodes: Vec::new(),
            
            // Timestamps
            timestamp,
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
            transaction_count: 0,
            total_revenue: 0,
            
            // Pricing
            pricing_config: PricingConfig {
                price_type: PriceType::Fixed,
                base_price: 0,
                usage_price: 0,
                access_price: 0,
                currency: b"NATIVE".to_vec(),
            },
            
            // Status
            status: AssetStatus::Private,
            updated_at: timestamp,
        }
    }
}

impl<AccountId: Clone + Encode> RightToken<AccountId> {
    /// Create a minimal RightToken with only required fields
    pub fn minimal(
        token_id: u32,
        right_type: RightType,
        holder: AccountId,
        issuer: AccountId,
        parent_asset_id: [u8; 32],
        current_time: u64,
        valid_until: Option<u64>
    ) -> Self {
        let certificate_id = Self::generate_certificate_id(&parent_asset_id, current_time, &issuer);
        
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
            valid_from: current_time,
            valid_until,
            
            // Ownership
            owner: holder,
            issuer,
            
            // Transaction info
            nonce: 0,
            
            // Parent asset reference
            parent_asset_id,
            
            // Status
            status: CertificateStatus::Active,
            
            // Signature
            signature: Vec::new(),
        }
    }
}

impl<AccountId> DataAsset<AccountId> {
    #[inline]
    pub fn asset_id(&self) -> [u8; 32] { // 返回[]copy不是&[]
        self.asset_id
    }
}

impl<AccountId> RightToken<AccountId> {
    #[inline]
    pub fn certificate_id(&self) -> [u8; 32] {
        self.certificate_id
    }

    #[inline]
    pub fn parent_asset_id(&self) -> [u8; 32] {
        self.parent_asset_id
    }
}