use std::collections::HashMap;
use std::marker::PhantomData;
use std::error::Error;

use hash_db::{HashDB, Hasher, AsHashDB, HashDBRef};
use kvdb::KeyValueDB;
use trie_db::DBValue;
use log::trace;

/// 用来存数据资产节点的 column
const ASSET_DB_COL: u32 = 0;

/// KVDB + 内存缓存 HashDB
pub struct KvdbHashDB<'a, H: Hasher> {
    kv: &'a dyn KeyValueDB,
    _marker: PhantomData<H>,
    cache: HashMap<Vec<u8>, DBValue>, // 内存缓存
}

impl<'a, H: Hasher> KvdbHashDB<'a, H> {
    pub fn new(kv: &'a dyn KeyValueDB) -> Self {
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
}

/// HashDB 实现
impl<'a, H: Hasher> HashDB<H, DBValue> for KvdbHashDB<'a, H>
where
    H::Out: AsRef<[u8]>,
{
    fn get(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> Option<DBValue> {
        println!("HashDB::get - key: {:?}, prefix: {:?}", key.as_ref(), prefix);
        
        // 全零hash返回 None
        if key.as_ref().iter().all(|&x| x == 0) {
            println!("HashDB::get - returning None for zero key");
            return None;
        }
        println!("key:{:?}, prefix:{:?}", key, prefix);

        let real_key = Self::make_prefixed_key(prefix, key.as_ref());

        // 先查内存缓存
        if let Some(v) = self.cache.get(&real_key) {
            trace!("cache hit, key:{:?}", key);
            return Some(v.clone());
        }

        // 再查 KVDB
        let result = self.kv.get(ASSET_DB_COL, &real_key).ok().flatten().map(|v| v.to_vec());
        println!("HashDB::get - DB lookup: {}", if result.is_some() { "found" } else { "not found" });
        result
    }

    fn contains(&self, key: &H::Out, prefix: (&[u8], Option<u8>)) -> bool {
        HashDB::get(self, key, prefix).is_some()
    }

    fn insert(&mut self, prefix: (&[u8], Option<u8>), value: &[u8]) -> H::Out {
        let hash = H::hash(value);
        println!("HashDB::insert - hash: {:?}, value_len: {}", hash.as_ref(), value.len());
        self.emplace(hash.clone(), prefix, value.to_vec());
        hash
    }

    fn emplace(&mut self, key: H::Out, prefix: (&[u8], Option<u8>), value: DBValue) {
        println!("HashDB::emplace - key: {:?}, prefix: {:?}, value_len: {}", 
             key.as_ref(), prefix, value.len());
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());

        // 写入 KVDB
        let mut tx = self.kv.transaction();
        tx.put(ASSET_DB_COL, &real_key, &value);
        self.kv.write(tx).expect("KVDB write failed");

        println!("HashDB::emplace - successfully wrote to DB");
        // 立即验证写入是否成功
        let verify = self.kv.get(ASSET_DB_COL, &real_key)
            .expect("KVDB get failed");
        
        if let Some(stored) = verify {
            println!("Verification SUCCESS - stored {} bytes", stored.len());
        } else {
            println!("Verification FAILED - data not found after write!");
        }

        // 写入缓存
        self.cache.insert(real_key.clone(), value.clone());
    }

    fn remove(&mut self, key: &H::Out, prefix: (&[u8], Option<u8>)) {
        println!("HashDB::remove called - key: {:?}", key.as_ref());
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());
        self.cache.remove(&real_key);

        let mut tx = self.kv.transaction();
        tx.delete(ASSET_DB_COL, &real_key);
        self.kv.write(tx).expect("KVDB delete failed");
    }
}

/// HashDBRef 实现
impl<'a, H: Hasher> HashDBRef<H, DBValue> for KvdbHashDB<'a, H>
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
impl<'a, H: Hasher> AsHashDB<H, DBValue> for KvdbHashDB<'a, H> {
    fn as_hash_db(&self) -> &dyn HashDB<H, DBValue> {
        self
    }

    fn as_hash_db_mut(&mut self) -> &mut dyn HashDB<H, DBValue> {
        self
    }
}

/// 变更收集器：收集所有读写操作，最后批量应用
/// 用于在不加载整个树的情况下进行增量修改
/// 
/// 新增功能：支持历史状态保护，防止删除仍被历史根引用的节点
pub struct ChangeCollector<'a, H: Hasher> {
    kv: &'a dyn KeyValueDB,
    pub changes: HashMap<Vec<u8>, Option<DBValue>>, // None 表示删除，公开以便调试
    preserve_history: bool, // 新增：是否保护历史状态
    _marker: PhantomData<H>,
}

impl<'a, H: Hasher> ChangeCollector<'a, H> {
    /// 创建新的 ChangeCollector，默认启用历史保护
    pub fn new(kv: &'a dyn KeyValueDB) -> Self {
        Self {
            kv,
            changes: HashMap::new(),
            preserve_history: true, // 默认启用历史保护
            _marker: PhantomData,
        }
    }

    /// 创建新的 ChangeCollector，可以选择是否启用历史保护
    /// 
    /// # 参数
    /// * `kv` - 键值数据库引用
    /// * `preserve_history` - 是否保护历史状态。如果为 true，则不会删除任何节点；如果为 false，则正常删除节点
    /// 
    /// # 注意
    /// 设置 `preserve_history` 为 false 可能会破坏历史状态的访问能力，请谨慎使用
    pub fn new_with_history_mode(kv: &'a dyn KeyValueDB, preserve_history: bool) -> Self {
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
    /// 
    /// 在历史保护模式下，删除操作会被跳过，只执行写入操作
    pub fn apply_changes(&self) -> Result<(), Box<dyn Error>> {
        if self.changes.is_empty() {
            return Ok(());
        }
        
        let mut tx = self.kv.transaction();
        let mut applied_count = 0;
        
        for (key, value_opt) in &self.changes {
            match value_opt {
                Some(value) => {
                    println!("Applying write: key len={}, value len={}", key.len(), value.len());
                    tx.put(ASSET_DB_COL, key, value);
                    applied_count += 1;
                },
                None => {
                    if self.preserve_history {
                        println!("Skipping delete (history preservation): key len={}", key.len());
                    } else {
                        println!("Applying delete: key len={}", key.len());
                        tx.delete(ASSET_DB_COL, key);
                        applied_count += 1;
                    }
                }
            }
        }
        
        self.kv.write(tx)?;
        println!("Applied {} changes to database (preserve_history: {})", applied_count, self.preserve_history);
        Ok(())
    }
}

impl<'a, H: Hasher> HashDB<H, DBValue> for ChangeCollector<'a, H>
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
        self.kv.get(ASSET_DB_COL, &real_key).ok().flatten().map(|v| v.to_vec())
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
        println!("ChangeCollector::emplace - recording write for key len={}, value len={}", real_key.len(), value.len());
        self.changes.insert(real_key, Some(value));
    }

    /// 记录删除操作
    /// 
    /// 在历史保护模式下，此方法仍会记录删除操作，但在 `apply_changes` 时会跳过实际删除
    /// 这样设计是为了保持 trie 库的正常工作流程，同时在应用阶段进行历史保护
    fn remove(&mut self, key: &H::Out, prefix: (&[u8], Option<u8>)) {
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());
        
        if self.preserve_history {
            println!("ChangeCollector::remove - recording delete for history-protected key len={} (will be skipped in apply_changes)", real_key.len());
        } else {
            println!("ChangeCollector::remove - recording delete for key len={}", real_key.len());
        }
        
        self.changes.insert(real_key, None);
    }
}

/// HashDBRef 实现
impl<'a, H: Hasher> HashDBRef<H, DBValue> for ChangeCollector<'a, H>
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
impl<'a, H: Hasher> AsHashDB<H, DBValue> for ChangeCollector<'a, H> {
    fn as_hash_db(&self) -> &dyn HashDB<H, DBValue> {
        self
    }

    fn as_hash_db_mut(&mut self) -> &mut dyn HashDB<H, DBValue> {
        self
    }
}