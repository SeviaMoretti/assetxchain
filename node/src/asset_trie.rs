use std::error::Error;
use std::collections::HashMap;

use kvdb::KeyValueDB;
use hash_db::{Hasher, HashDB, AsHashDB};
use trie_db::{
    TrieMut, Trie, TrieDBMutBuilder, TrieDBBuilder, TrieLayout, TrieHash, DBValue,
};
use memory_db::{MemoryDB, HashKey};

use crate::kvdb_hashdb::KvdbHashDB;

const ASSET_DB_COL: u32 = 0;

pub struct AssetTrie<'a, L: TrieLayout>
where
    L::Hash: Hasher,
{
    kv: &'a dyn KeyValueDB,
    root: TrieHash<L>,
    _marker: std::marker::PhantomData<L>,
}

impl<'a, L> AssetTrie<'a, L>
where
    L: TrieLayout + 'static,
    L::Hash: Hasher + 'static,
    <<L as TrieLayout>::Hash as Hasher>::Out: 'static,
{
    pub fn new(kv: &'a dyn KeyValueDB, initial_root: TrieHash<L>) -> Self {
        Self {
            kv,
            root: initial_root,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn root(&self) -> TrieHash<L> {
        self.root.clone()
    }

    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<TrieHash<L>, Box<dyn Error>> {
        self.batch_insert(std::iter::once((key.to_vec(), value.to_vec())))
    }

    pub fn batch_insert<I>(&mut self, items: I) -> Result<TrieHash<L>, Box<dyn Error>>
    where
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
    {
        let items: Vec<(Vec<u8>, Vec<u8>)> = items.into_iter().collect();
        
        if items.is_empty() {
            return Ok(self.root.clone());
        }

        // 检查是否为空树
        let is_empty_tree = self.root == Default::default() || 
                        self.root.as_ref().iter().all(|&x| x == 0);

        let all_items = if is_empty_tree {
            // 空树情况：直接使用新数据
            println!("Inserting into empty tree");
            items
        } else {
            // 非空树情况：先读取现有数据，然后合并新数据
            println!("Inserting into existing tree, merging with current data");
            
            let existing_items = {
                let hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
                let trie = TrieDBBuilder::<L>::new(&hashdb, &self.root).build();
                
                let mut existing = std::collections::HashMap::new();
                let mut iter = trie.iter()?;
                while let Some(result) = iter.next() {
                    let (key, value) = result?;
                    existing.insert(key, value.to_vec());
                }
                existing
            };

            // 合并现有数据和新数据（新数据覆盖现有数据）
            let mut combined = existing_items;
            for (key, value) in items {
                combined.insert(key, value);
            }
            
            println!("Combined {} existing items with new items", combined.len());
            combined.into_iter().collect()
        };

        // 重建trie - 从空树开始
        let mut memdb = MemoryDB::<L::Hash, HashKey<L::Hash>, DBValue>::default();
        let mut root_local: TrieHash<L> = Default::default();

        {
            let mut trie = TrieDBMutBuilder::<L>::new(&mut memdb, &mut root_local).build();
            for (k, v) in all_items {
                trie.insert(&k, &v)?;
            }
        }
        
        // 手动将 memdb 中的节点写入实际数据库
        let mut hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        for (hash, (value, rc)) in memdb.drain() {
            if rc > 0 {
                println!("Writing node to DB: hash={:?}, len={}", hash, value.len());
                hashdb.emplace(hash, (&[], None), value);
            }
        }

        println!("Final root after insert: {:?}", root_local);
        self.root = root_local;
        Ok(self.root.clone())
    }

    pub fn remove(&mut self, key: &[u8]) -> Result<TrieHash<L>, Box<dyn Error>> {
        self.batch_remove(std::iter::once(key.to_vec()))
    }

    // 从现有的数据库状态开始：先将现有trie数据复制到内存数据库中，然后删除指定键
    pub fn batch_remove<I>(&mut self, keys: I) -> Result<TrieHash<L>, Box<dyn Error>>
    where
        I: IntoIterator<Item = Vec<u8>>,
    {
        let is_empty_tree = self.root == Default::default() || 
                        self.root.as_ref().iter().all(|&x| x == 0);
        
        if is_empty_tree {
            // 空树没有东西可删除
            return Ok(self.root.clone());
        }

        let keys_to_remove: std::collections::HashSet<Vec<u8>> = keys.into_iter().collect();
        
        if keys_to_remove.is_empty() {
            return Ok(self.root.clone());
        }

        // 读取现有trie的所有数据
        let existing_items = {
            let hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
            let trie = TrieDBBuilder::<L>::new(&hashdb, &self.root).build();
            
            let mut items = Vec::new();
            let mut iter = trie.iter()?;
            while let Some(result) = iter.next() {
                let (key, value) = result?;
                if !keys_to_remove.contains(&key) {
                    items.push((key, value.to_vec()));
                }
            }
            items
        };

        println!("Found {} existing items, {} to keep after removal", 
                existing_items.len() + keys_to_remove.len(), existing_items.len());

        // 如果没有项目要保留，直接返回空树
        if existing_items.is_empty() {
            println!("No items to keep, setting root to empty tree");
            self.root = Default::default();
            return Ok(self.root.clone());
        }

        // 重建trie - 使用与 batch_insert 完全相同的模式
        let mut memdb = MemoryDB::<L::Hash, HashKey<L::Hash>, DBValue>::default();
        let mut root_local: TrieHash<L> = Default::default(); // 从空树开始

        {
            let mut trie = TrieDBMutBuilder::<L>::new(&mut memdb, &mut root_local).build();
            for (k, v) in existing_items {
                trie.insert(&k, &v)?;
            }
        }
        
        // 手动将 memdb 中的节点写入实际数据库 - 与 batch_insert 相同
        let mut hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        for (hash, (value, rc)) in memdb.drain() {
            if rc > 0 {
                println!("Writing node to DB: hash={:?}, len={}", hash, value.len());
                hashdb.emplace(hash, (&[], None), value);
            }
        }

        println!("Final root after removal: {:?}", root_local);
        self.root = root_local;
        Ok(self.root.clone())
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<DBValue>, Box<dyn Error>> {
        // 空树直接返回 None
        if self.root == Default::default() || self.root.as_ref().iter().all(|&x| x == 0) {
            return Ok(None);
        }

        let hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        
        println!("Getting key: {:?} with root: {:?}", key, self.root);

        // 不确定根节点是否存在，不使用from_existing，不然可能报错
        let trie = TrieDBBuilder::<L>::new(&hashdb, &self.root).build();
        match trie.get(key) {
            Ok(opt) => Ok(opt.map(|v| v.to_vec())),
            Err(e) => {
                println!("Error getting key: {:?}", e);
                Err(Box::new(e) as Box<dyn Error>)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use kvdb_memorydb; 
    use kvdb_rocksdb::{Database as RocksDb, DatabaseConfig};
    use reference_trie::NoExtensionLayout as Layout;
    use trie_db::TrieHash;

    // 内存中
     #[test]
    fn test_asset_trie_basic_ops() {
        let kv = kvdb_memorydb::create(1);
        let mut trie = AssetTrie::<Layout>::new(&kv, Default::default());

        // 单条插入
        let key = b"key1";
        let value = b"value1";
        let root = trie.insert(key, value).unwrap();
        assert_eq!(trie.get(key).unwrap().unwrap(), value);

        // 单条删除
        let root2 = trie.remove(key).unwrap();
        assert!(trie.get(key).unwrap().is_none());

        // 批量插入
        let items = vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
            (b"k3".to_vec(), b"v3".to_vec()),
        ];
        trie.batch_insert(items.clone()).unwrap();
        for (k, v) in &items {
            assert_eq!(trie.get(k).unwrap().unwrap(), *v);
        }

        // 批量删除
        let keys = vec![b"k1".to_vec(), b"k2".to_vec()];
        trie.batch_remove(keys).unwrap();
        assert!(trie.get(b"k1").unwrap().is_none());
        assert!(trie.get(b"k2").unwrap().is_none());
        assert_eq!(trie.get(b"k3").unwrap().unwrap(), b"v3");
    }

    // 存到文件中
    #[test]
    fn test_asset_trie_disk_basic_ops() {
        // 项目根目录下的 node 文件夹
        let node_dir = Path::new("./testdata/diskt");
        // 清理之前的测试数据（若存在）
        if node_dir.exists() {
            let _ = fs::remove_dir_all(&node_dir);
        }
        fs::create_dir_all(&node_dir).expect("create node dir failed");

        // **磁盘存储配置** - 设置合理的内存预算
        let mut config = DatabaseConfig::with_columns(1); // 1 列就够做测试
        config.memory_budget.insert(0, 32); // 为列族0设置32MB内存预算
        config.max_open_files = 512; // 适合测试的文件句柄数量
        
        let db = RocksDb::open(&config, &node_dir).expect("open rocksdb failed");
        
        // initial root：使用默认空 root
        let initial_root: TrieHash<Layout> = Default::default();
        let mut asset_trie = AssetTrie::<Layout>::new(&db, initial_root);
        
        println!("initial root: {:?}\n", asset_trie.root());

        // ---- 单条插入与读取
        let key1 = b"key1";
        let val1 = b"value1";
        let _root_after_insert = asset_trie.insert(key1, val1)
            .expect("insert failed");
        let got = asset_trie.get(key1).expect("get failed");
        assert!(got.is_some(), "value should exist after insert");
        assert_eq!(got.unwrap(), val1.to_vec());

        // ---- 单条删除
        let _root_after_remove = asset_trie.remove(key1).expect("remove failed");
        let got_after_remove = asset_trie.get(key1).expect("get after remove failed");
        assert!(got_after_remove.is_none(), "value should be removed");

        // ---- 批量插入
        let items = vec![
            (b"aa".to_vec(), b"v_aa".to_vec()),
            (b"bb".to_vec(), b"v_bb".to_vec()),
            (b"cc".to_vec(), b"v_cc".to_vec()),
        ];
        let _root_after_batch = asset_trie.batch_insert(items.clone()).expect("batch_insert failed");
        for (k, v) in items.iter() {
            let got = asset_trie.get(k).expect("get in batch failed");
            assert!(got.is_some(), "batch inserted key should exist");
            assert_eq!(got.unwrap(), v.clone());
        }

        // ---- 批量删除
        let keys_to_remove = vec![b"aa".to_vec(), b"cc".to_vec()];
        let _root_after_batch_remove = asset_trie.batch_remove(keys_to_remove.clone()).expect("batch_remove failed");
        
        // 验证删除结果
        let got_aa = asset_trie.get(b"aa").expect("get aa after batch remove failed");
        assert!(got_aa.is_none());
        let got_bb = asset_trie.get(b"bb").expect("get bb after batch remove failed");
        assert!(got_bb.is_some());
        assert_eq!(got_bb.unwrap(), b"v_bb".to_vec());
        let got_cc = asset_trie.get(b"cc").expect("get cc after batch remove failed");
        assert!(got_cc.is_none());

        // 清理：drop 并删除 node 目录（测试结束）
        drop(asset_trie);
        drop(db);
        let _ = fs::remove_dir_all(&node_dir);
    }

    // 数据库关上，再打开
    #[test]
    fn test_asset_trie_disk_persistence() {
        // **持久化验证测试**
        let node_dir = Path::new("./testdata/persistence");
        
        // 清理之前的测试数据
        if node_dir.exists() {
            let _ = fs::remove_dir_all(&node_dir);
        }
        fs::create_dir_all(&node_dir).expect("create persistence dir failed");

        let final_root_hash = {
            // **第一阶段：写入数据并记录根哈希**
            let mut config = DatabaseConfig::with_columns(1);
            config.memory_budget.insert(0, 32); // 32MB内存预算
            config.max_open_files = 512;
            
            let db = RocksDb::open(&config, &node_dir).expect("Failed to open RocksDB");
            let mut asset_trie = AssetTrie::<Layout>::new(&db, Default::default());
            
            // 写入持久化测试数据
            let persistent_items = vec![
                (b"persistent_key1".to_vec(), b"persistent_value1".to_vec()),
                (b"persistent_key2".to_vec(), b"persistent_value2".to_vec()),
                (b"persistent_key3".to_vec(), b"persistent_value3".to_vec()),
            ];
            
            let root_hash = asset_trie.batch_insert(persistent_items.clone())
                .expect("Failed to insert persistent data");
            
            println!("Phase 1: Inserted data with root hash: {:?}", root_hash);
            
            // 验证数据写入成功
            for (key, value) in &persistent_items {
                let retrieved = asset_trie.get(key).expect("Failed to get persistent data");
                assert!(retrieved.is_some(), "Persistent data should exist");
                assert_eq!(retrieved.unwrap(), *value);
            }
            
            println!("Phase 1: Data verification successful");
            
            // 显式关闭数据库
            drop(asset_trie);
            drop(db);
            println!("Phase 1: Database closed");
            
            root_hash
        }; // 第一阶段结束，数据库已关闭

        {
            // **第二阶段：重新打开数据库并验证数据持久化**
            println!("\nPhase 2: Reopening database...");
            
            let mut config = DatabaseConfig::with_columns(1);
            config.memory_budget.insert(0, 32);
            config.max_open_files = 512;
            
            let db = RocksDb::open(&config, &node_dir).expect("Failed to reopen RocksDB");
            
            // 使用保存的根哈希重新创建 AssetTrie
            let asset_trie = AssetTrie::<Layout>::new(&db, final_root_hash);
            
            println!("Phase 2: AssetTrie recreated with root: {:?}", final_root_hash);
            
            // 验证持久化数据仍然存在且正确
            let persistent_items = vec![
                (b"persistent_key1".as_slice(), b"persistent_value1".as_slice()),
                (b"persistent_key2".as_slice(), b"persistent_value2".as_slice()),
                (b"persistent_key3".as_slice(), b"persistent_value3".as_slice()),
            ];
            
            for (key, expected_value) in &persistent_items {
                let retrieved = asset_trie.get(key).expect("Failed to get data after reopen");
                assert!(retrieved.is_some(), "Data should persist after database reopen");
                assert_eq!(retrieved.unwrap(), expected_value.to_vec());
                println!("Phase 2: Verified key {:?} = {:?}", 
                         String::from_utf8_lossy(key), 
                         String::from_utf8_lossy(expected_value));
            }
            
            println!("Phase 2: All persistent data verified successfully!");
            
            // 清理
            drop(asset_trie);
            drop(db);
        }
        
        // 最终清理测试目录
        let _ = fs::remove_dir_all(&node_dir);
        println!("Persistence test completed successfully!");
    }

    // 往空树插入，再往非空树插入
    #[test]
    fn test_batch_insert_on_existing_tree() {
        let kv = kvdb_memorydb::create(1);
        let mut trie = AssetTrie::<Layout>::new(&kv, Default::default());

        // 第一次插入
        let items1 = vec![
            (b"key1".to_vec(), b"value1".to_vec()),
            (b"key2".to_vec(), b"value2".to_vec()),
        ];
        trie.batch_insert(items1).unwrap();
        
        // 验证第一次插入
        assert_eq!(trie.get(b"key1").unwrap().unwrap(), b"value1");
        assert_eq!(trie.get(b"key2").unwrap().unwrap(), b"value2");

        // 第二次插入到非空树
        let items2 = vec![
            (b"key2".to_vec(), b"value2_updated".to_vec()), // 覆盖现有
            (b"key3".to_vec(), b"value3".to_vec()),         // 新增
        ];
        trie.batch_insert(items2).unwrap();
        
        // 验证合并结果
        assert_eq!(trie.get(b"key1").unwrap().unwrap(), b"value1");        // 保留
        assert_eq!(trie.get(b"key2").unwrap().unwrap(), b"value2_updated"); // 更新
        assert_eq!(trie.get(b"key3").unwrap().unwrap(), b"value3");        // 新增
    }
}
