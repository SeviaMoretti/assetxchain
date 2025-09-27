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
        
        // 空树全零 hash 返回 None
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
pub struct ChangeCollector<'a, H: Hasher> {
    kv: &'a dyn KeyValueDB,
    pub changes: HashMap<Vec<u8>, Option<DBValue>>, // None 表示删除，公开以便调试
    _marker: PhantomData<H>,
}

impl<'a, H: Hasher> ChangeCollector<'a, H> {
    pub fn new(kv: &'a dyn KeyValueDB) -> Self {
        Self {
            kv,
            changes: HashMap::new(),
            _marker: PhantomData,
        }
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
    
    pub fn apply_changes(&self) -> Result<(), Box<dyn Error>> {
        if self.changes.is_empty() {
            return Ok(());
        }
        
        let mut tx = self.kv.transaction();
        for (key, value_opt) in &self.changes {
            match value_opt {
                Some(value) => {
                    println!("Applying write: key len={}, value len={}", key.len(), value.len());
                    tx.put(ASSET_DB_COL, key, value);
                },
                None => {
                    println!("Applying delete: key len={}", key.len());
                    tx.delete(ASSET_DB_COL, key);
                }
            }
        }
        self.kv.write(tx)?;
        println!("Applied {} changes to database", self.changes.len());
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

    fn remove(&mut self, key: &H::Out, prefix: (&[u8], Option<u8>)) {
        let real_key = Self::make_prefixed_key(prefix, key.as_ref());
        println!("ChangeCollector::remove - recording delete for key len={}", real_key.len());
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