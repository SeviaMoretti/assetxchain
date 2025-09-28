use std::sync::{Arc, RwLock, Mutex};
use std::collections::HashMap;
use sp_core::{H256, H160};
use kvdb::KeyValueDB;
use reference_trie::NoExtensionLayout as Layout;
use trie_db::{TrieHash, DBValue};
use log::{info, warn, error, debug};
use codec::{Encode, Decode};
use sp_runtime::traits::{BlakeTwo256, Hash as HashT};

use crate::mpt::AssetTrie;

use crate::dataasset::{
    DataAsset, RightToken, AssetStatus, CertificateStatus,
    RightType, ASSET_PROTOCOL_VERSION, RIGHT_TOKEN_PROTOCOL_VERSION
};

/// 简化版双层 MPT 管理器
/// 
/// 架构说明：
/// - 主资产树：存储所有数据资产的状态
/// - 权证子树：每个数据资产对应一个子树，存储该资产的权证状态
/// 
/// 存储策略：
/// - 主树键：asset_id (32字节) -> DataAsset
/// - 子树键：certificate_id (4字节little-endian) -> RightToken
pub struct SimplifiedDualLayerMptManager {
    /// 数据库引用
    db: Arc<dyn KeyValueDB>,
    
    /// 主资产状态树
    main_asset_tree: Arc<Mutex<AssetTrie<Layout>>>,
    
    /// 权证子树缓存 (asset_id -> certificate_tree)
    certificate_trees: Arc<RwLock<HashMap<[u8; 32], Arc<Mutex<AssetTrie<Layout>>>>>>,
    
    /// 当前主树根哈希
    current_main_root: Arc<RwLock<TrieHash<Layout>>>,
    
    /// 各资产权证树根哈希缓存 (asset_id -> cert_tree_root)
    certificate_roots: Arc<RwLock<HashMap<[u8; 32], TrieHash<Layout>>>>,
    
    /// token_id到asset_id的映射
    token_id_to_asset_id: Arc<RwLock<HashMap<u32, [u8; 32]>>>,
    
    /// 下一个可用的token_id
    next_token_id: Arc<RwLock<u32>>,
}

impl SimplifiedDualLayerMptManager {
    /// 创建新的双层 MPT 管理器
    pub fn new(db: Arc<dyn KeyValueDB>) -> Self {
        let main_tree = AssetTrie::new(db.clone(), Default::default());
        
        Self {
            db,
            main_asset_tree: Arc::new(Mutex::new(main_tree)),
            certificate_trees: Arc::new(RwLock::new(HashMap::new())),
            current_main_root: Arc::new(RwLock::new(Default::default())),
            certificate_roots: Arc::new(RwLock::new(HashMap::new())),
            token_id_to_asset_id: Arc::new(RwLock::new(HashMap::new())),
            next_token_id: Arc::new(RwLock::new(0)),
        }
    }

    /// 从现有根哈希创建管理器（用于恢复状态）
    pub fn from_root(db: Arc<dyn KeyValueDB>, root: TrieHash<Layout>) -> Self {
        let main_tree = AssetTrie::new(db.clone(), root);
        
        Self {
            db,
            main_asset_tree: Arc::new(Mutex::new(main_tree)),
            certificate_trees: Arc::new(RwLock::new(HashMap::new())),
            current_main_root: Arc::new(RwLock::new(root)),
            certificate_roots: Arc::new(RwLock::new(HashMap::new())),
            token_id_to_asset_id: Arc::new(RwLock::new(HashMap::new())),
            next_token_id: Arc::new(RwLock::new(0)),
        }
    }

    /// 获取当前主资产树根哈希
    pub fn get_main_root(&self) -> TrieHash<Layout> {
        *self.current_main_root.read().unwrap()
    }

    /// 获取指定资产的权证树根哈希
    pub fn get_certificate_root(&self, asset_id: &[u8; 32]) -> TrieHash<Layout> {
        self.certificate_roots
            .read()
            .unwrap()
            .get(asset_id)
            .copied()
            .unwrap_or_default()
    }

    /// 注册新的数据资产
    pub fn register_asset(
        &self,
        mut asset: DataAsset,
    ) -> Result<([u8; 32], TrieHash<Layout>), Box<dyn std::error::Error>> {
        info!("Registering new data asset: {:?}", 
              String::from_utf8_lossy(&asset.name));

        // 分配token_id
        asset.token_id = self.allocate_token_id();
        
        // 生成asset_id（如果还没有生成）
        if asset.asset_id == [0u8; 32] {
            asset.asset_id = DataAsset::generate_asset_id(
                &asset.owner,
                asset.timestamp,
                &asset.raw_data_hash
            );
        }

        // 设置初始状态
        asset.status = AssetStatus::Active;
        asset.children_root = [0u8; 32]; // 初始化为空的权证树
        asset.updated_at = Self::current_timestamp();

        // 记录token_id到asset_id的映射
        self.token_id_to_asset_id
            .write()
            .unwrap()
            .insert(asset.token_id, asset.asset_id);

        // 为新资产初始化空的权证树
        self.initialize_certificate_tree(&asset.asset_id)?;

        // 将资产保存到主树
        let new_root = self.insert_asset(&asset.asset_id, &asset)?;
        
        info!("Asset registered - ID: {:?}, token_id: {}", 
              asset.asset_id, asset.token_id);
        
        Ok((asset.asset_id, new_root))
    }

    /// 发行权证
    pub fn issue_certificate(
        &self,
        asset_id: &[u8; 32],
        holder: H160,
        right_type: RightType,
        valid_until: Option<u64>,
    ) -> Result<(u32, TrieHash<Layout>), Box<dyn std::error::Error>> {
        info!("Issuing certificate for asset: {:?}", asset_id);

        // 验证父资产是否存在且活跃
        let asset = self.get_asset_state_by_id(asset_id)
            .ok_or("Parent asset not found")?;

        if !asset.is_active() {
            return Err("Parent asset is not active".into());
        }

        // 生成证书ID
        let certificate_id = self.get_next_certificate_id(asset_id)?;
        
        // 创建权证
        let mut certificate = RightToken {
            version: RIGHT_TOKEN_PROTOCOL_VERSION.as_bytes().to_vec(),
            certificate_id,
            right_type,
            create_time: Self::current_timestamp(),
            confirm_time: Self::current_timestamp(),
            valid_from: Self::current_timestamp(),
            valid_until,
            owner: holder,
            issuer: asset.owner,
            parent_asset_id: *asset_id,
            parent_asset_token_id: asset.token_id,
            status: CertificateStatus::Active,
            ..Default::default()
        };

        // 生成token_id
        certificate.token_id = RightToken::generate_token_id(
            asset.token_id, 
            certificate.certificate_id
        );

        // 保存权证到子树
        let new_cert_root = self.insert_certificate(asset_id, &certificate)?;
        
        // 更新主树中资产的权证树根
        self.update_asset_certificate_root(asset_id, new_cert_root)?;

        info!("Certificate issued with ID: {}", certificate.certificate_id);
        
        Ok((certificate.certificate_id, new_cert_root))
    }

    /// 转移资产所有权
    pub fn transfer_asset(
        &self,
        asset_id: &[u8; 32],
        new_owner: H160,
        current_owner: &H160,
    ) -> Result<TrieHash<Layout>, Box<dyn std::error::Error>> {
        info!("Transferring asset {:?} to {:?}", asset_id, new_owner);

        let mut asset = self.get_asset_state_by_id(asset_id)
            .ok_or("Asset not found")?;

        // 验证转移权限
        if asset.owner != *current_owner {
            return Err("Only asset owner can transfer ownership".into());
        }

        if asset.is_locked() {
            return Err("Cannot transfer locked asset".into());
        }

        // 更新资产信息
        asset.owner = new_owner;
        asset.nonce += 1;
        asset.transaction_count += 1;
        asset.confirm_time = Self::current_timestamp();
        asset.updated_at = Self::current_timestamp();

        let new_root = self.insert_asset(asset_id, &asset)?;

        info!("Asset ownership transferred successfully");
        
        Ok(new_root)
    }

    /// 撤销权证
    pub fn revoke_certificate(
        &self,
        asset_id: &[u8; 32],
        certificate_id: u32,
        revoker: &H160,
    ) -> Result<TrieHash<Layout>, Box<dyn std::error::Error>> {
        info!("Revoking certificate {} for asset {:?}", certificate_id, asset_id);

        // 验证权限
        if let Some(asset) = self.get_asset_state_by_id(asset_id) {
            if let Some(cert) = self.get_certificate_state(asset_id, certificate_id) {
                if asset.owner != *revoker && cert.owner != *revoker {
                    return Err("Insufficient permissions to revoke certificate".into());
                }
            } else {
                return Err("Certificate not found".into());
            }
        } else {
            return Err("Asset not found".into());
        }

        // 从权证树中删除
        let new_cert_root = self.remove_certificate(asset_id, certificate_id)?;
        
        // 更新主树中资产的权证树根
        self.update_asset_certificate_root(asset_id, new_cert_root)?;

        info!("Certificate {} revoked successfully", certificate_id);
        
        Ok(new_cert_root)
    }

    /// 更新权证状态（例如标记为过期）
    pub fn update_certificate_status(
        &self,
        asset_id: &[u8; 32],
        certificate_id: u32,
        new_status: CertificateStatus,
    ) -> Result<TrieHash<Layout>, Box<dyn std::error::Error>> {
        let mut certificate = self.get_certificate_state(asset_id, certificate_id)
            .ok_or("Certificate not found")?;

        certificate.status = new_status;
        
        let new_cert_root = self.insert_certificate(asset_id, &certificate)?;
        self.update_asset_certificate_root(asset_id, new_cert_root)?;

        Ok(new_cert_root)
    }

    /// 查询资产状态（通过asset_id）
    pub fn get_asset_state_by_id(&self, asset_id: &[u8; 32]) -> Option<DataAsset> {
        let key = asset_id.to_vec();
        let main_tree = self.main_asset_tree.lock().unwrap();
        
        match main_tree.get(&key) {
            Ok(Some(data)) => {
                match DataAsset::decode(&mut &data[..]) {
                    Ok(asset) => Some(asset),
                    Err(e) => {
                        warn!("Failed to decode asset state for {:?}: {:?}", asset_id, e);
                        None
                    }
                }
            },
            _ => {
                debug!("Asset {:?} not found in main tree", asset_id);
                None
            }
        }
    }

    /// 查询资产状态（通过token_id）
    pub fn get_asset_state_by_token_id(&self, token_id: u32) -> Option<DataAsset> {
        let asset_id = self.token_id_to_asset_id.read().unwrap().get(&token_id).copied()?;
        self.get_asset_state_by_id(&asset_id)
    }

    /// 查询权证状态
    pub fn get_certificate_state(&self, asset_id: &[u8; 32], certificate_id: u32) -> Option<RightToken> {
        let cert_tree = self.get_or_create_certificate_tree(asset_id).ok()?;
        let key = Self::make_certificate_key(certificate_id);
        
        let cert_tree_guard = cert_tree.lock().unwrap();
        match cert_tree_guard.get(&key) {
            Ok(Some(data)) => {
                match RightToken::decode(&mut &data[..]) {
                    Ok(cert) => Some(cert),
                    Err(e) => {
                        warn!("Failed to decode certificate state for {}: {:?}", certificate_id, e);
                        None
                    }
                }
            },
            _ => {
                debug!("Certificate {} for asset {:?} not found", certificate_id, asset_id);
                None
            }
        }
    }

    /// 获取资产的所有权证
    pub fn get_asset_certificates(&self, asset_id: &[u8; 32]) -> Result<Vec<RightToken>, Box<dyn std::error::Error>> {
        let cert_tree = self.get_or_create_certificate_tree(asset_id)?;
        let cert_tree_guard = cert_tree.lock().unwrap();
        
        let all_certs_data = cert_tree_guard.iter_all()?;
        let mut certificates = Vec::new();
        
        for (_, cert_data) in all_certs_data {
            if let Ok(cert) = RightToken::decode(&mut &cert_data[..]) {
                certificates.push(cert);
            }
        }
        
        // 按证书ID排序
        certificates.sort_by_key(|cert| cert.certificate_id);
        
        Ok(certificates)
    }

    /// 获取用户持有的权证列表
    pub fn get_user_certificates(&self, user: &H160) -> Result<Vec<(DataAsset, RightToken)>, Box<dyn std::error::Error>> {
        let main_tree = self.main_asset_tree.lock().unwrap();
        let all_assets_data = main_tree.iter_all()?;
        let mut user_certificates = Vec::new();

        for (_, asset_data) in all_assets_data {
            if let Ok(asset) = DataAsset::decode(&mut &asset_data[..]) {
                let certificates = self.get_asset_certificates(&asset.asset_id)?;
                
                for cert in certificates {
                    if cert.owner == *user {
                        user_certificates.push((asset.clone(), cert));
                    }
                }
            }
        }

        Ok(user_certificates)
    }

    /// 获取用户拥有的资产列表
    pub fn get_user_assets(&self, user: &H160) -> Result<Vec<DataAsset>, Box<dyn std::error::Error>> {
        let main_tree = self.main_asset_tree.lock().unwrap();
        let all_assets_data = main_tree.iter_all()?;
        let mut user_assets = Vec::new();

        for (_, asset_data) in all_assets_data {
            if let Ok(asset) = DataAsset::decode(&mut &asset_data[..]) {
                if asset.owner == *user {
                    user_assets.push(asset);
                }
            }
        }

        Ok(user_assets)
    }

    // 内部辅助方法

    /// 分配新的token_id
    fn allocate_token_id(&self) -> u32 {
        let mut next_id = self.next_token_id.write().unwrap();
        let id = *next_id;
        *next_id += 1;
        id
    }

    /// 插入资产到主树
    fn insert_asset(&self, asset_id: &[u8; 32], asset: &DataAsset) -> Result<TrieHash<Layout>, Box<dyn std::error::Error>> {
        let key = asset_id.to_vec();
        let value = asset.encode();
        
        let mut main_tree = self.main_asset_tree.lock().unwrap();
        main_tree.insert(&key, &value)?;
        
        let new_root = main_tree.root();
        *self.current_main_root.write().unwrap() = new_root;
        
        Ok(new_root)
    }

    /// 插入权证到子树
    fn insert_certificate(&self, asset_id: &[u8; 32], certificate: &RightToken) -> Result<TrieHash<Layout>, Box<dyn std::error::Error>> {
        let cert_tree = self.get_or_create_certificate_tree(asset_id)?;
        let key = Self::make_certificate_key(certificate.certificate_id);
        let value = certificate.encode();
        
        let mut cert_tree_guard = cert_tree.lock().unwrap();
        cert_tree_guard.insert(&key, &value)?;
        
        let new_root = cert_tree_guard.root();
        self.certificate_roots.write().unwrap().insert(*asset_id, new_root);
        
        Ok(new_root)
    }

    /// 从子树删除权证
    fn remove_certificate(&self, asset_id: &[u8; 32], certificate_id: u32) -> Result<TrieHash<Layout>, Box<dyn std::error::Error>> {
        let cert_tree = self.get_or_create_certificate_tree(asset_id)?;
        let key = Self::make_certificate_key(certificate_id);
        
        let mut cert_tree_guard = cert_tree.lock().unwrap();
        cert_tree_guard.remove(&key)?;
        
        let new_root = cert_tree_guard.root();
        self.certificate_roots.write().unwrap().insert(*asset_id, new_root);
        
        Ok(new_root)
    }

    /// 初始化资产的权证树
    fn initialize_certificate_tree(&self, asset_id: &[u8; 32]) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Initializing certificate tree for asset {:?}", asset_id);
        
        let cert_tree = AssetTrie::new(self.db.clone(), Default::default());
        let cert_tree_arc = Arc::new(Mutex::new(cert_tree));
        
        self.certificate_trees
            .write()
            .unwrap()
            .insert(*asset_id, cert_tree_arc);
            
        self.certificate_roots
            .write()
            .unwrap()
            .insert(*asset_id, Default::default());
            
        Ok(())
    }

    /// 获取或创建权证树
    fn get_or_create_certificate_tree(&self, asset_id: &[u8; 32]) -> Result<Arc<Mutex<AssetTrie<Layout>>>, Box<dyn std::error::Error>> {
        // 首先检查缓存
        {
            let trees_guard = self.certificate_trees.read().unwrap();
            if let Some(tree) = trees_guard.get(asset_id) {
                return Ok(Arc::clone(tree));
            }
        }

        // 如果不存在，初始化新的权证树
        self.initialize_certificate_tree(asset_id)?;
        
        let trees_guard = self.certificate_trees.read().unwrap();
        let tree = trees_guard.get(asset_id)
            .ok_or("Failed to create certificate tree")?;
        Ok(Arc::clone(tree))
    }

    /// 更新主树中资产的权证树根
    fn update_asset_certificate_root(
        &self, 
        asset_id: &[u8; 32], 
        cert_root: TrieHash<Layout>
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!("Updating certificate root for asset {:?} in main tree", asset_id);

        let mut asset = self.get_asset_state_by_id(asset_id)
            .ok_or("Asset not found when updating certificate root")?;

        // 更新权证树根
        asset.children_root = cert_root.as_ref().try_into()
            .unwrap_or([0u8; 32]);
        asset.updated_at = Self::current_timestamp();

        // 保存更新后的资产
        self.insert_asset(asset_id, &asset)?;

        Ok(())
    }

    /// 获取下一个证书ID
    fn get_next_certificate_id(&self, asset_id: &[u8; 32]) -> Result<u32, Box<dyn std::error::Error>> {
        let cert_tree = self.get_or_create_certificate_tree(asset_id)?;
        let cert_tree_guard = cert_tree.lock().unwrap();
        
        let all_certs = cert_tree_guard.iter_all()?;
        let mut max_id = 0u32;
        
        for (key, _) in all_certs {
            if key.len() >= 4 {
                let cert_id = u32::from_le_bytes([key[0], key[1], key[2], key[3]]);
                if cert_id > max_id {
                    max_id = cert_id;
                }
            }
        }
        
        Ok(max_id + 1)
    }

    /// 生成权证存储键
    fn make_certificate_key(certificate_id: u32) -> Vec<u8> {
        certificate_id.to_le_bytes().to_vec()
    }

    /// 获取当前时间戳
    fn current_timestamp() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kvdb_memorydb;
    use std::sync::Arc;
    use crate::dataasset::*;

    fn create_test_asset(owner: H160, name: &str) -> DataAsset {
        let timestamp = SimplifiedDualLayerMptManager::current_timestamp();
        let raw_data_hash = H256::from_low_u64_be(12345);
        
        DataAsset {
            version: ASSET_PROTOCOL_VERSION.as_bytes().to_vec(),
            name: name.as_bytes().to_vec(),
            description: b"A test data asset".to_vec(),
            quantity: b"100MB".to_vec(),
            raw_data_hash,
            owner,
            timestamp,
            confirm_time: timestamp,
            ..Default::default()
        }
    }

    #[test]
    fn test_asset_registration() {
        let db = Arc::new(kvdb_memorydb::create(1));
        let manager = SimplifiedDualLayerMptManager::new(db);

        let owner = H160::from_low_u64_be(1);
        let asset = create_test_asset(owner, "Test Asset");
        
        let result = manager.register_asset(asset.clone());
        assert!(result.is_ok(), "Asset registration failed: {:?}", result);
        
        let (asset_id, new_root) = result.unwrap();
        assert_ne!(new_root, TrieHash::<Layout>::default());
        assert_eq!(manager.get_main_root(), new_root);

        // 验证资产可以被查询
        let retrieved_asset = manager.get_asset_state_by_id(&asset_id);
        assert!(retrieved_asset.is_some());
        
        let retrieved_asset = retrieved_asset.unwrap();
        assert_eq!(retrieved_asset.name, asset.name);
        assert_eq!(retrieved_asset.owner, asset.owner);
        assert_eq!(retrieved_asset.token_id, 0); // 第一个资产的token_id应该是0
    }

    #[test]
    fn test_certificate_issuance() {
        let db = Arc::new(kvdb_memorydb::create(1));
        let manager = SimplifiedDualLayerMptManager::new(db);

        // 注册资产
        let owner = H160::from_low_u64_be(1);
        let asset = create_test_asset(owner, "Test Asset");
        let (asset_id, _) = manager.register_asset(asset).unwrap();

        // 发行权证
        let certificate_holder = H160::from_low_u64_be(2);
        let result = manager.issue_certificate(
            &asset_id, 
            certificate_holder, 
            RightType::Usage, 
            None
        );
        assert!(result.is_ok(), "Certificate issuance failed: {:?}", result);
        
        let (cert_id, cert_root) = result.unwrap();
        assert_ne!(cert_root, TrieHash::<Layout>::default());

        // 验证权证可以被查询
        let retrieved_cert = manager.get_certificate_state(&asset_id, cert_id);
        assert!(retrieved_cert.is_some());
        
        let retrieved_cert = retrieved_cert.unwrap();
        assert_eq!(retrieved_cert.owner, certificate_holder);
        assert_eq!(retrieved_cert.parent_asset_id, asset_id);
        assert_eq!(retrieved_cert.right_type, RightType::Usage);
    }

    #[test]
    fn test_asset_transfer() {
        let db = Arc::new(kvdb_memorydb::create(1));
        let manager = SimplifiedDualLayerMptManager::new(db);

        // 注册资产
        let original_owner = H160::from_low_u64_be(1);
        let asset = create_test_asset(original_owner, "Test Asset");
        let (asset_id, _) = manager.register_asset(asset).unwrap();

        // 转移资产
        let new_owner = H160::from_low_u64_be(2);
        let result = manager.transfer_asset(&asset_id, new_owner, &original_owner);
        assert!(result.is_ok(), "Asset transfer failed: {:?}", result);

        // 验证所有权变更
        let updated_asset = manager.get_asset_state_by_id(&asset_id).unwrap();
        assert_eq!(updated_asset.owner, new_owner);
        assert_eq!(updated_asset.nonce, 1);
    }

    #[test]
    fn test_certificate_revocation() {
        let db = Arc::new(kvdb_memorydb::create(1));
        let manager = SimplifiedDualLayerMptManager::new(db);

        // 注册资产并发行权证
        let owner = H160::from_low_u64_be(1);
        let asset = create_test_asset(owner, "Test Asset");
        let (asset_id, _) = manager.register_asset(asset).unwrap();

        let certificate_holder = H160::from_low_u64_be(2);
        let (cert_id, _) = manager.issue_certificate(
            &asset_id, 
            certificate_holder, 
            RightType::Access, 
            None
        ).unwrap();

        // 撤销权证
        let result = manager.revoke_certificate(&asset_id, cert_id, &owner);
        assert!(result.is_ok(), "Certificate revocation failed: {:?}", result);

        // 验证权证已被删除
        let revoked_cert = manager.get_certificate_state(&asset_id, cert_id);
        assert!(revoked_cert.is_none());
    }

    #[test]
    fn test_get_user_assets_and_certificates() {
        let db = Arc::new(kvdb_memorydb::create(1));
        let manager = SimplifiedDualLayerMptManager::new(db);

        let user1 = H160::from_low_u64_be(1);
        let user2 = H160::from_low_u64_be(2);

        // 用户1注册资产
        let asset1 = create_test_asset(user1, "User1 Asset");
        let (asset1_id, _) = manager.register_asset(asset1).unwrap();

        // 为用户2发行权证
        manager.issue_certificate(&asset1_id, user2, RightType::Usage, None).unwrap();

        // 测试获取用户资产
        let user1_assets = manager.get_user_assets(&user1).unwrap();
        assert_eq!(user1_assets.len(), 1);
        assert_eq!(user1_assets[0].name, b"User1 Asset");

        let user2_assets = manager.get_user_assets(&user2).unwrap();
        assert_eq!(user2_assets.len(), 0);

        // 测试获取用户权证
        let user2_certs = manager.get_user_certificates(&user2).unwrap();
        assert_eq!(user2_certs.len(), 1);
        assert_eq!(user2_certs[0].1.right_type, RightType::Usage);
    }
}