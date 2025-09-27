// 资产元数据数据结构
use sp_core::H256;

pub struct Dataasset {
    id: u64,
    name: String,
    owner: H256,
    children_root: H256,
    cid: String,
    nounce: u64,   
}

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