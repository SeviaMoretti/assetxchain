use codec::{Encode, Decode};
use sp_std::vec::Vec;
use sp_core::{H256, H160};
use scale_info::TypeInfo;

// 版本常量定义
pub const ASSET_PROTOCOL_VERSION: &str = "1.0";
pub const RIGHT_TOKEN_PROTOCOL_VERSION: &str = "1.0";

/// 简化的数据资产状态结构
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct DataAsset {
    // 协议版本号
    pub version: Vec<u8>,
    
    // 资产唯一标识，通过owner + timestamp + hash生成
    pub asset_id: [u8; 32],
    
    // 通过合约注册资产时分配的递增ID
    pub token_id: u32,
    
    // 基本信息
    pub name: Vec<u8>,
    pub description: Vec<u8>,
    pub quantity: Vec<u8>, // 原始数据的量
    pub labels: Vec<Vec<u8>>, // 标签
    
    // 数据特征
    pub statistical_characteristic: Vec<u8>, // 统计特征
    pub analyzing_feature: Vec<u8>, // 分析特征
    pub integrity: Vec<u8>, // 完整度
    pub raw_data_hash: H256, // 原始数据哈希
    
    // 所有权信息
    pub owner: H160, // 所有者地址
    
    // IPFS存储信息
    pub metadata_cid: Vec<u8>, // 元数据IPFS哈希
    pub data_cid_merkle_nodes: Vec<MerkleNode>, // CID默克尔树节点
    
    // 时间戳和签名
    pub timestamp: u64, // 创建时间
    pub confirm_time: u64, // 确权时间
    pub signature: Vec<u8>, // 创建签名
    
    // 交易和状态信息
    pub nonce: u32, // 元证交易次数
    pub is_locked: bool, // 元证合并后的上锁标志位
    
    // 加密信息
    pub encryption_info: EncryptionInfo,
    
    // 权证子树根哈希
    pub children_root: [u8; 32],
    
    // 统计信息
    pub view_count: u64,
    pub download_count: u64,
    pub transaction_count: u64,
    pub total_revenue: u128,
    
    // 简化的定价配置
    pub pricing_config: PricingConfig,
    
    // 资产状态
    pub status: AssetStatus,
    
    // 更新时间
    pub updated_at: u64,
}

/// 简化的数据权证结构
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct RightToken {
    // 协议版本号
    pub version: Vec<u8>,
    
    // 权证ID，格式：父资产token_id|权证序号
    pub token_id: Vec<u8>,
    
    // 权证唯一标识
    pub certificate_id: u32,
    
    // 权证类型（简化版）
    pub right_type: RightType,
    
    // 时间信息
    pub create_time: u64, // 权证创建时间
    pub confirm_time: u64, // 确权时间
    pub valid_from: u64, // 权证生效时间
    pub valid_until: Option<u64>, // 权证失效时间
    
    // 所有权信息
    pub owner: H160, // 权证所有者
    pub issuer: H160, // 权证发行者
    
    // 交易信息
    pub nonce: u32, // 权证交易次数
    
    // 关联的父资产
    pub parent_asset_id: [u8; 32], // 父元证ID
    pub parent_asset_token_id: u32, // 父元证的token_id
    
    // 权证状态
    pub status: CertificateStatus,
    
    // 溯源信息
    pub right_token_from: Option<Vec<u8>>, // 权证来源
    
    // 签名信息
    pub signature: Vec<u8>, // 权证签名
}

/// 加密信息结构
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct EncryptionInfo {
    pub algorithm: Vec<u8>, // 加密算法名称
    pub key_length: u32, // 密钥长度
    pub parameters_hash: H256, // 加密参数的哈希
    pub is_encrypted: bool, // 是否加密
}

/// Merkle树节点
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct MerkleNode {
    pub hash: H256,
    pub is_leaf: bool,
    pub data: Option<Vec<u8>>, // 叶子节点可能包含实际数据
}

/// 简化的权证类型枚举
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum RightType {
    Usage = 1,  // 使用权
    Access = 2, // 访问权
}

/// 简化的资产状态枚举
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum AssetStatus {
    Active = 1, // 活跃
    Locked = 2, // 锁定
}

/// 简化的权证状态枚举
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub enum CertificateStatus {
    Active = 1,  // 活跃
    Expired = 2, // 已过期
}

/// 简化的定价配置
#[derive(Encode, Decode, Clone, PartialEq, Eq, Debug, TypeInfo)]
pub struct PricingConfig {
    pub base_price: u128, // 基础价格
    pub currency: Vec<u8>, // 货币类型
}

// 实现默认值
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

impl DataAsset {
    /// 生成资产ID
    pub fn generate_asset_id(owner: &H160, timestamp: u64, data_hash: &H256) -> [u8; 32] {
        use sp_runtime::traits::{BlakeTwo256, Hash as HashT};
        
        let mut input = Vec::new();
        input.extend_from_slice(owner.as_bytes());
        input.extend_from_slice(&timestamp.to_le_bytes());
        input.extend_from_slice(data_hash.as_bytes());
        
        let hash = BlakeTwo256::hash(&input);
        hash.into()
    }
    
    /// 检查资产是否已锁定
    pub fn is_locked(&self) -> bool {
        self.is_locked || self.status == AssetStatus::Locked
    }
    
    /// 检查资产是否活跃
    pub fn is_active(&self) -> bool {
        self.status == AssetStatus::Active && !self.is_locked()
    }
}

impl RightToken {
    /// 生成权证token_id
    pub fn generate_token_id(parent_token_id: u32, certificate_sequence: u32) -> Vec<u8> {
        format!("{}|{}", parent_token_id, certificate_sequence).into_bytes()
    }
    
    /// 检查权证是否有效
    pub fn is_valid(&self, current_time: u64) -> bool {
        self.status == CertificateStatus::Active &&
        current_time >= self.valid_from &&
        self.valid_until.map_or(true, |until| current_time <= until)
    }
    
    /// 检查是否过期
    pub fn is_expired(&self, current_time: u64) -> bool {
        self.valid_until.map_or(false, |until| current_time > until)
    }
}

// 数据权证

/*
type DataAsset struct {
	Version string //版本号,参考ipv4、ipv6版本号.定义一个version的常量,newAsset时赋值，在专利中描述，（资产管理协议）

	//ID []byte // 资产唯一标识(key，可通过key从mpt中获取资产信息)，由owner、timestamp、hash生成

	TokenID string `json:"tokenID"` // 通过合约注册资产时，该资产是整个交易链第i个资产，tokenID为i（i=0;i++）

	Name        string
	Description string
	Quantity    string   //原始数据的量
	Labels      []string `json:"labels"` // 标签 应该是id，资产类型

	StatisticalCharacteristic []byte //统计特征
	AnalyzingFeature          []byte //分析特征，注意与统计特征的区别（有什么区别呢？）
	Integrity                 string //完整度

	Hash  string // RawDataHash(原始数据哈希)，未加密的分块文件的merkle root
	Owner string `json:"owner"` // 所有者地址（hex编码）
	//CId   string    `json:"metadata"`  // 元数据IPFS哈希
	CID []trie.MerkleNode //CID默克尔树(只存叶子节点？+根节点);用json存应该也行（[]byte）

	Timestamp      time.Time `json:"timestamp"` // 创建时间
	Signature      string    `json:"signature"` // 创建签名
	Nonce          int       //元证交易次数，单调增
	EncryptionInfo string    //对原始数据进行加密的算法等信息（肯定不能有加密密钥，可以有hash）可能是[]byte类型
	IsLocked       bool      //元证合并后，上锁标志位
	// ConfirmTime    time.Time // 确权时间戳（当前所有者的确权时间）,溯源有用，从confirmTime最近的区块开始溯源
	ChildrenRoot []byte
	/* 初始为nil，数据权证的根节点哈希(MPT) 应该是common.Hash-->[32]byte,
	但是node.go中使用的是[]byte,之后得重构 */
}

// 数据资产_数据权证_元数据
type RightToken struct {
	Version string // 版本号,参考ipv4、ipv6版本号，定义一个version的常量
	// 符号'|'表示拼接
	TokenID string // 和对应元证的tokenID有关系（元证的tokenID为i，权证的tokenID为i|j）

	RightType      int       // 权证类型（例如：1、使用权，2、访问权）
	CreateTime     time.Time // 权证创建时间
	ConfirmTime    time.Time // 确权时间（当前所有者的确权时间）
	Owner          string    // 权证所有者,初始为元证的owner
	Nounce         int       // 权证交易次数，单调增，初始为0
	RightTokenFrom string    // 权证来源？和确权时间冲突（可能会多余）
	ParentAssetKey string    // 元证ID或key
	// ParnetHash string //元证的hash
	// InterestLimit []type // 权限范围
}
*/ 