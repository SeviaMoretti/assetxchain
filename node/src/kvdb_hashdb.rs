use std::collections::HashMap;
use std::marker::PhantomData;
use std::error::Error;
use std::sync::Arc;

use hash_db::{HashDB, Hasher, AsHashDB, HashDBRef};
use kvdb::KeyValueDB;
use trie_db::DBValue;
use log::{trace, debug, warn};

/// 用来存数据资产节点的 column
const ASSET_DB_COL: u32 = 0;

/// 改进的 KVDB + 内存缓存 HashDB
pub struct KvdbHashDB<H: Hasher> {
    kv: Arc<dyn KeyValueDB>,
    _marker: PhantomData<H>,
    cache: HashMap<Vec<u8>, DBValue>, // 内存缓存
}

impl<H: Hasher> KvdbHashDB<H> {
    pub fn new(kv: Arc<dyn KeyValueDB>) -> Self {
        Self {
            kv,
            _marker: PhantomData,
            cache: HashMap::new(),
        }
    }

    /// 构造存储用的最终 key = prefix.0 (+ prefix.1) + 哈希值
    fn make_prefixed_key(prefix: (&[u8], Option<u8>), key: &[u8]) -> Vec<u8> {
        let mut real_key = Vec::with_capacity(prefix.0.len() + 1 + key.len());
        real_key.extend_from_slice(prefix.0);
        if let Some(tag) = prefix.1 {
            real_key.push(tag);
        }
        real_key.extend_from_slice(key);
        real_key
    }

    /// 清空缓存
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// 获取缓存大小
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

/// HashDB 实现
impl<H: Hasher> HashDB<H, DBValue> for KvdbHashDB<H>
where
    H::Out: AsRef<[u8]>,
{
    fn get(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> Option<DBValue> {
        debug!("HashDB::get - key: {:?}, prefix: {:?}", 
               key.as_ref().get(0..8).unwrap_or(&[]), prefix);
        
        // 全零hash返回 None
        if key.as_ref().iter().all(|&x| x == 0) {
            debug!("HashDB::get - returning None for zero key");
            return None;
        }

        let real_key = Self::make_prefixed_key(prefix, key.as_ref());

        // 先查内存缓存
        if let Some(v) = self.cache.get(&real_key) {
            trace!("Cache hit for key");
            return Some(v.clone());
        }

        // 再查 KVDB
        match self.kv.get(ASSET_DB_COL, &real_key) {
            Ok(Some(data)) => {
                debug!("HashDB::get - found in DB, size: {}", data.len());
                Some(data.to_vec())
            },
            Ok(None) => {
                debug!("HashDB::get - not found in DB");
                None
            },
            Err(e) => {
                warn!("HashDB::get - DB error: {:?}", e);
                None
            }
        }
    }

    fn contains(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> bool {
        HashDB::get(self, key, prefix).is_some()
    }

    fn insert(&mut self, prefix: (&[u8], Option<u8>), value: &[u8]) -> H::Out {
        let hash = H::hash(value);
        debug!("HashDB::insert - hash: {:?}, value_len: {}", 
               hash.as_ref().get(0..8).unwrap_or(&[]), value.len());
        self.emplace(hash.clone(), prefix, value.to_vec());
        hash
    }

    fn emplace(&mut self, key: H::Out, prefix: (&[u8], Option<u8>), value: DBValue) {
        debug!("HashDB::emplace - key: {:?}, prefix: {:?}, value_len: {}", 
               key.as_ref().get(0..8).unwrap_or(&[]), prefix, value.len());
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());

        // 写入 KVDB
        let mut tx = self.kv.transaction();
        tx.put(ASSET_DB_COL, &real_key, &value);
        
        if let Err(e) = self.kv.write(tx) {
            warn!("KVDB write failed: {:?}", e);
            return;
        }

        debug!("HashDB::emplace - successfully wrote to DB");
        
        // 验证写入（可选，在调试时启用）
        if cfg!(debug_assertions) {
            match self.kv.get(ASSET_DB_COL, &real_key) {
                Ok(Some(stored)) => {
                    debug!("Verification SUCCESS - stored {} bytes", stored.len());
                },
                Ok(None) => {
                    warn!("Verification FAILED - data not found after write!");
                },
                Err(e) => {
                    warn!("Verification ERROR: {:?}", e);
                }
            }
        }

        // 写入缓存
        self.cache.insert(real_key, value);
    }

    fn remove(&mut self, key: &H::Out, prefix: (&[u8], Option<u8>)) {
        debug!("HashDB::remove called - key: {:?}", 
               key.as_ref().get(0..8).unwrap_or(&[]));
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());
        
        // 从缓存中移除
        self.cache.remove(&real_key);

        // 从数据库中删除
        let mut tx = self.kv.transaction();
        tx.delete(ASSET_DB_COL, &real_key);
        
        if let Err(e) = self.kv.write(tx) {
            warn!("KVDB delete failed: {:?}", e);
        }
    }
}

/// HashDBRef 实现
impl<H: Hasher> HashDBRef<H, DBValue> for KvdbHashDB<H>
where
    H::Out: AsRef<[u8]>,
{
    fn get(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> Option<DBValue> {
        HashDB::get(self, key, prefix)
    }

    fn contains(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> bool {
        HashDB::contains(self, key, prefix)
    }
}

/// AsHashDB 实现
impl<H: Hasher> AsHashDB<H, DBValue> for KvdbHashDB<H> {
    fn as_hash_db(&self) -> &dyn HashDB<H, DBValue> {
        self
    }

    fn as_hash_db_mut(&mut self) -> &mut dyn HashDB<H, DBValue> {
        self
    }
}

/// 改进的变更收集器，支持历史状态保护和批量操作优化
pub struct ChangeCollector<H: Hasher> {
    kv: Arc<dyn KeyValueDB>,
    pub changes: HashMap<Vec<u8>, Option<DBValue>>, // 公开以便调试
    preserve_history: bool,
    _marker: PhantomData<H>,
}

impl<H: Hasher> ChangeCollector<H> {
    /// 创建新的 ChangeCollector，默认启用历史保护
    pub fn new(kv: Arc<dyn KeyValueDB>) -> Self {
        Self {
            kv,
            changes: HashMap::new(),
            preserve_history: true,
            _marker: PhantomData,
        }
    }

    /// 创建支持配置历史保护模式的 ChangeCollector
    pub fn new_with_history_mode(kv: Arc<dyn KeyValueDB>, preserve_history: bool) -> Self {
        Self {
            kv,
            changes: HashMap::new(),
            preserve_history,
            _marker: PhantomData,
        }
    }

    /// 获取当前的历史保护设置
    pub fn is_preserving_history(&self) -> bool {
        self.preserve_history
    }

    /// 设置历史保护模式
    pub fn set_preserve_history(&mut self, preserve: bool) {
        self.preserve_history = preserve;
    }
    
    fn make_prefixed_key(prefix: (&[u8], Option<u8>), key: &[u8]) -> Vec<u8> {
        let mut real_key = Vec::with_capacity(prefix.0.len() + 1 + key.len());
        real_key.extend_from_slice(prefix.0);
        if let Some(tag) = prefix.1 {
            real_key.push(tag);
        }
        real_key.extend_from_slice(key);
        real_key
    }
    
    /// 应用所有收集到的变更到数据库
    pub fn apply_changes(&self) -> Result<(), Box<dyn Error>> {
        if self.changes.is_empty() {
            debug!("No changes to apply");
            return Ok(());
        }
        
        let mut tx = self.kv.transaction();
        let mut write_count = 0;
        let mut delete_count = 0;
        let mut skip_count = 0;
        
        for (key, value_opt) in &self.changes {
            match value_opt {
                Some(value) => {
                    debug!("Applying write: key len={}, value len={}", key.len(), value.len());
                    tx.put(ASSET_DB_COL, key, value);
                    write_count += 1;
                },
                None => {
                    if self.preserve_history {
                        debug!("Skipping delete (history preservation): key len={}", key.len());
                        skip_count += 1;
                    } else {
                        debug!("Applying delete: key len={}", key.len());
                        tx.delete(ASSET_DB_COL, key);
                        delete_count += 1;
                    }
                }
            }
        }
        
        self.kv.write(tx)?;
        
        debug!(
            "Applied changes - writes: {}, deletes: {}, skipped: {} (preserve_history: {})", 
            write_count, delete_count, skip_count, self.preserve_history
        );
        
        Ok(())
    }

    /// 清空收集的变更
    pub fn clear_changes(&mut self) {
        self.changes.clear();
    }

    /// 获取变更数量统计
    pub fn change_stats(&self) -> (usize, usize, usize) {
        let mut writes = 0;
        let mut deletes = 0;
        
        for value_opt in self.changes.values() {
            match value_opt {
                Some(_) => writes += 1,
                None => deletes += 1,
            }
        }
        
        (writes, deletes, self.changes.len())
    }
}

impl<H: Hasher> HashDB<H, DBValue> for ChangeCollector<H>
where
    H::Out: AsRef<[u8]>,
{
    fn get(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> Option<DBValue> {
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());
        
        // 先查变更记录
        if let Some(change) = self.changes.get(&real_key) {
            return change.clone();
        }
        
        // 再查原始数据库
        match self.kv.get(ASSET_DB_COL, &real_key) {
            Ok(opt) => opt.map(|v| v.to_vec()),
            Err(e) => {
                warn!("ChangeCollector::get - DB error: {:?}", e);
                None
            }
        }
    }

    fn contains(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> bool {
        HashDB::get(self, key, prefix).is_some()
    }

    fn insert(&mut self, prefix: (&[u8], Option<u8>), value: &[u8]) -> H::Out {
        let hash = H::hash(value);
        self.emplace(hash.clone(), prefix, value.to_vec());
        hash
    }

    fn emplace(&mut self, key: H::Out, prefix: (&[u8], Option<u8>), value: DBValue) {
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());
        debug!("ChangeCollector::emplace - recording write for key len={}, value len={}", 
               real_key.len(), value.len());
        self.changes.insert(real_key, Some(value));
    }

    fn remove(&mut self, key: &H::Out, prefix: (&[u8], Option<u8>)) {
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());
        
        if self.preserve_history {
            debug!("ChangeCollector::remove - recording delete for history-protected key len={} (will be skipped in apply_changes)", real_key.len());
        } else {
            debug!("ChangeCollector::remove - recording delete for key len={}", real_key.len());
        }
        
        self.changes.insert(real_key, None);
    }
}

/// HashDBRef 实现
impl<H: Hasher> HashDBRef<H, DBValue> for ChangeCollector<H>
where
    H::Out: AsRef<[u8]>,
{
    fn get(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> Option<DBValue> {
        HashDB::get(self, key, prefix)
    }

    fn contains(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> bool {
        HashDB::contains(self, key, prefix)
    }
}

/// AsHashDB 实现
impl<H: Hasher> AsHashDB<H, DBValue> for ChangeCollector<H> {
    fn as_hash_db(&self) -> &dyn HashDB<H, DBValue> {
        self
    }

    fn as_hash_db_mut(&mut self) -> &mut dyn HashDB<H, DBValue> {
        self
    }
}