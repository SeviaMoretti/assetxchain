use std::error::Error;
use std::collections::HashMap;
use std::sync::Arc;

use kvdb::KeyValueDB;
use hash_db::{Hasher, HashDB, AsHashDB};
use trie_db::{
    TrieMut, Trie, TrieDBMutBuilder, TrieDBBuilder, TrieLayout, TrieHash, DBValue,
};
use memory_db::{MemoryDB, HashKey};

use crate::kvdb_hashdb::{KvdbHashDB, ChangeCollector};

const ASSET_DB_COL: u32 = 0;

/// 改进的 AssetTrie，解决生命周期问题
pub struct AssetTrie<L: TrieLayout>
where
    L::Hash: Hasher,
{
    db: Arc<dyn KeyValueDB>,
    root: TrieHash<L>,
    _marker: std::marker::PhantomData<L>,
}

// proof相关的方法为实现
impl<L> AssetTrie<L>
where
    L: TrieLayout + 'static,
    L::Hash: Hasher + 'static,
    <<L as TrieLayout>::Hash as Hasher>::Out: 'static,
{
    /// 创建新的 AssetTrie
    pub fn new(db: Arc<dyn KeyValueDB>, initial_root: TrieHash<L>) -> Self {
        Self {
            db,
            root: initial_root,
            _marker: std::marker::PhantomData,
        }
    }

    /// 获取当前根哈希
    pub fn root(&self) -> TrieHash<L> {
        self.root.clone()
    }

    /// 设置根哈希
    pub fn set_root(&mut self, new_root: TrieHash<L>) {
        self.root = new_root;
    }

    /// 插入单个键值对
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<TrieHash<L>, Box<dyn Error>> {
        self.batch_insert(vec![(key.to_vec(), value.to_vec())])
    }

    /// 批量插入
    pub fn batch_insert(&mut self, items: Vec<(Vec<u8>, Vec<u8>)>) -> Result<TrieHash<L>, Box<dyn Error>> {
        if items.is_empty() {
            return Ok(self.root.clone());
        }

        let is_empty_tree = self.is_empty_root();

        if is_empty_tree {
            // 空树：使用 MemoryDB 创建
            self.create_new_tree(items)
        } else {
            // 非空树：使用 ChangeCollector 增量更新
            self.incremental_update(items, Vec::new())
        }
    }

    /// 删除单个键
    pub fn remove(&mut self, key: &[u8]) -> Result<TrieHash<L>, Box<dyn Error>> {
        self.batch_remove(vec![key.to_vec()])
    }

    /// 批量删除
    pub fn batch_remove(&mut self, keys: Vec<Vec<u8>>) -> Result<TrieHash<L>, Box<dyn Error>> {
        if keys.is_empty() {
            return Ok(self.root.clone());
        }

        if self.is_empty_root() {
            // 空树没有东西可删除
            return Ok(self.root.clone());
        }

        // 非空树：使用 ChangeCollector 增量更新
        self.incremental_update(Vec::new(), keys)
    }

    /// 批量更新（插入和删除）
    pub fn batch_update(
        &mut self, 
        inserts: Vec<(Vec<u8>, Vec<u8>)>, 
        deletes: Vec<Vec<u8>>
    ) -> Result<TrieHash<L>, Box<dyn Error>> {
        if inserts.is_empty() && deletes.is_empty() {
            return Ok(self.root.clone());
        }

        if self.is_empty_root() && !deletes.is_empty() {
            // 空树没有东西可删除，只处理插入
            return self.create_new_tree(inserts);
        }

        if self.is_empty_root() {
            // 空树：使用 MemoryDB 创建
            self.create_new_tree(inserts)
        } else {
            // 非空树：使用 ChangeCollector 增量更新
            self.incremental_update(inserts, deletes)
        }
    }

    /// 获取键对应的值
    pub fn get(&self, key: &[u8]) -> Result<Option<DBValue>, Box<dyn Error>> {
        if self.is_empty_root() {
            return Ok(None);
        }

        let hashdb = KvdbHashDB::<L::Hash>::new(self.db.clone());
        let trie = TrieDBBuilder::<L>::new(&hashdb, &self.root).build();
        
        match trie.get(key) {
            Ok(opt) => Ok(opt.map(|v| v.to_vec())),
            Err(e) => Err(Box::new(e) as Box<dyn Error>),
        }
    }

    /// 检查键是否存在
    pub fn contains(&self, key: &[u8]) -> Result<bool, Box<dyn Error>> {
        Ok(self.get(key)?.is_some())
    }

    /// 获取所有键值对（用于调试或小型树）
    pub fn iter_all(&self) -> Result<HashMap<Vec<u8>, Vec<u8>>, Box<dyn Error>> {
        if self.is_empty_root() {
            return Ok(HashMap::new());
        }

        let mut result = HashMap::new();
        let hashdb = KvdbHashDB::<L::Hash>::new(self.db.clone());
        let trie = TrieDBBuilder::<L>::new(&hashdb, &self.root).build();
        
        if let Ok(mut iter) = trie.iter() {
            while let Some(item) = iter.next() {
                if let Ok((key, value)) = item {
                    result.insert(key, value.to_vec());
                }
            }
        }
        
        Ok(result)
    }

    /// 判断是否为空根
    fn is_empty_root(&self) -> bool {
        self.root == Default::default() || 
        self.root.as_ref().iter().all(|&x| x == 0)
    }

    /// 使用 MemoryDB 创建新树（避免 ChangeCollector 在空树时的问题）
    fn create_new_tree(&mut self, items: Vec<(Vec<u8>, Vec<u8>)>) -> Result<TrieHash<L>, Box<dyn Error>> {
        println!("Creating new tree with {} items", items.len());
        
        let mut memdb = MemoryDB::<L::Hash, HashKey<L::Hash>, DBValue>::default();
        let mut root_local: TrieHash<L> = Default::default();

        {
            let mut trie = TrieDBMutBuilder::<L>::new(&mut memdb, &mut root_local).build();
            for (k, v) in items {
                trie.insert(&k, &v)?;
            }
        }
        
        // 使用与 ChangeCollector 兼容的格式写入数据库
        self.write_memdb_with_correct_format(memdb)?;

        self.root = root_local;
        println!("New tree created, root: {:?}", self.root);
        Ok(self.root.clone())
    }

    /// 使用 ChangeCollector 进行增量更新
    fn incremental_update(
        &mut self, 
        inserts: Vec<(Vec<u8>, Vec<u8>)>, 
        deletes: Vec<Vec<u8>>
    ) -> Result<TrieHash<L>, Box<dyn Error>> {
        println!("Incremental update: {} inserts, {} deletes", inserts.len(), deletes.len());
        
        // 验证根节点是否存在
        let hashdb = KvdbHashDB::<L::Hash>::new(self.db.clone());
        if !hashdb.contains(&self.root, (&[], None)) {
            return Err(format!("Root node not found in database: {:?}", self.root).into());
        }

        // 对于纯删除操作且只有一个键的情况，检查是否会导致空树
        if !deletes.is_empty() && inserts.is_empty() && deletes.len() == 1 {
            // 检查当前树是否只有这一个键
            let current_data = self.iter_all()?;
            if current_data.len() == 1 && current_data.contains_key(&deletes[0]) {
                println!("Deleting the only key, resulting in empty tree");
                self.root = Default::default();
                return Ok(self.root.clone());
            }
        }

        // 尝试使用 ChangeCollector 进行更新
        let mut change_collector = ChangeCollector::<L::Hash>::new_with_history_mode(self.db.clone(), true);
        let mut root_local: TrieHash<L> = self.root.clone();

        println!("Starting trie operations with root: {:?}", root_local);
        
        let result = {
            let mut trie = TrieDBMutBuilder::<L>::from_existing(&mut change_collector, &mut root_local).build();
            
            // 执行插入操作
            for (k, v) in &inserts {
                println!("Inserting key: {:?}", k);
                trie.insert(&k, &v)?;
            }
            
            // 执行删除操作
            for k in &deletes {
                println!("Removing key: {:?}", k);
                trie.remove(&k)?;
            }
            
            Ok(())
        };

        if let Err(e) = result {
            return Err(e);
        }
        
        println!("After operations, root: {:?}", root_local);
        
        // 打印调试信息
        let (writes, dels, total) = change_collector.change_stats();
        println!("Changes collected - writes: {}, deletes: {}, total: {}", writes, dels, total);
        
        // 如果没有记录到写入操作但根哈希发生了变化，这表明 ChangeCollector 有问题
        if writes == 0 && root_local != self.root {
            println!("Warning: Root changed but no writes recorded. Using fallback method.");
            return self.fallback_update(inserts, deletes);
        }
        
        // 应用所有变更到数据库
        change_collector.apply_changes()?;

        // 验证新根是否可访问
        if root_local != Default::default() && !root_local.as_ref().iter().all(|&x| x == 0) {
            let verification_hashdb = KvdbHashDB::<L::Hash>::new(self.db.clone());
            if !verification_hashdb.contains(&root_local, (&[], None)) {
                println!("Root verification failed. Using fallback method.");
                return self.fallback_update(inserts, deletes);
            }
            println!("Root verification: SUCCESS");
        }

        // 检查结果是否为空树
        let is_empty_after = root_local == Default::default() || 
                            root_local.as_ref().iter().all(|&x| x == 0);
        
        if is_empty_after {
            self.root = Default::default();
            println!("Result: empty tree");
        } else {
            self.root = root_local;
            println!("Result: non-empty tree");
        }

        println!("Incremental update completed, root: {:?}", self.root);
        Ok(self.root.clone())
    }

    /// 后备更新方法：使用 MemoryDB 重新创建子树
    fn fallback_update(
        &mut self, 
        inserts: Vec<(Vec<u8>, Vec<u8>)>, 
        deletes: Vec<Vec<u8>>
    ) -> Result<TrieHash<L>, Box<dyn Error>> {
        println!("Using fallback update method");
        
        // 读取当前的所有数据
        let mut current_data = self.iter_all()?;
        println!("Current tree has {} items", current_data.len());
        
        // 应用删除操作
        for delete_key in deletes {
            current_data.remove(&delete_key);
        }
        
        // 应用插入操作
        for (insert_key, insert_value) in inserts {
            current_data.insert(insert_key, insert_value);
        }
        
        println!("After changes: {} items", current_data.len());
        
        // 如果结果为空，返回空树
        if current_data.is_empty() {
            self.root = Default::default();
            println!("Fallback result: empty tree");
            return Ok(self.root.clone());
        }
        
        // 使用内存数据库重建树
        let mut memdb = MemoryDB::<L::Hash, HashKey<L::Hash>, DBValue>::default();
        let mut new_root: TrieHash<L> = Default::default();

        {
            let mut trie = TrieDBMutBuilder::<L>::new(&mut memdb, &mut new_root).build();
            for (k, v) in current_data {
                trie.insert(&k, &v)?;
            }
        }
        
        // 写入到持久存储
        self.write_memdb_with_correct_format(memdb)?;

        self.root = new_root;
        println!("Fallback completed, new root: {:?}", self.root);
        Ok(self.root.clone())
    }

    /// 构造存储用的最终 key = prefix.0 (+ prefix.1) + 哈希值
    fn make_prefixed_key(&self, prefix: (&[u8], Option<u8>), key: &[u8]) -> Vec<u8> {
        let mut real_key = Vec::with_capacity(prefix.0.len() + 1 + key.len());
        real_key.extend_from_slice(prefix.0);
        if let Some(tag) = prefix.1 {
            real_key.push(tag);
        }
        real_key.extend_from_slice(key);
        real_key
    }

    /// 使用与 ChangeCollector 兼容的格式写入 MemoryDB 数据
    fn write_memdb_with_correct_format(&self, mut memdb: MemoryDB<L::Hash, HashKey<L::Hash>, DBValue>) -> Result<(), Box<dyn Error>> {
        let mut transaction = self.db.transaction();
        
        for (hash, (value, rc)) in memdb.drain() {
            if rc > 0 {
                // 使用与 KvdbHashDB 和 ChangeCollector 相同的键格式
                let prefixed_key = self.make_prefixed_key((&[], None), hash.as_ref());
                transaction.put(ASSET_DB_COL, &prefixed_key, &value);
                println!("Writing to DB: key_len={}, value_len={}", prefixed_key.len(), value.len());
            }
        }
        
        self.db.write(transaction).map_err(|e| Box::new(e) as Box<dyn Error>)?;
        println!("MemoryDB data committed to persistent storage");
        Ok(())
    }
}

// 为了支持克隆，我们需要实现 Clone
impl<L> Clone for AssetTrie<L>
where
    L: TrieLayout,
    L::Hash: Hasher,
{
    fn clone(&self) -> Self {
        Self {
            db: Arc::clone(&self.db),
            root: self.root.clone(),
            _marker: std::marker::PhantomData,
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
    use std::sync::Arc;

    #[test]
    fn test_asset_trie_basic() {
        let kv = Arc::new(kvdb_memorydb::create(1));
        let mut trie = AssetTrie::<Layout>::new(kv, Default::default());

        // 插入单个项目
        let key = b"test_key";
        let value = b"test_value";
        let result = trie.insert(key, value);
        println!("Insert result: {:?}", result);
        assert!(result.is_ok(), "Insert failed: {:?}", result);
        
        // 验证插入
        let get_result = trie.get(key);
        println!("Get result: {:?}", get_result);
        assert!(get_result.is_ok(), "Get failed: {:?}", get_result);
        assert_eq!(get_result.unwrap().unwrap(), value);
        
        assert!(trie.contains(key).unwrap());
        
        // 删除项目
        let remove_result = trie.remove(key);
        println!("Remove result: {:?}", remove_result);
        assert!(remove_result.is_ok(), "Remove failed: {:?}", remove_result);
        
        assert!(trie.get(key).unwrap().is_none());
        assert!(!trie.contains(key).unwrap());
    }

    #[test]
    fn test_batch_operations() {
        let kv = Arc::new(kvdb_memorydb::create(1));
        let mut trie = AssetTrie::<Layout>::new(kv, Default::default());

        // 批量插入
        let items = vec![
            (b"key1".to_vec(), b"value1".to_vec()),
            (b"key2".to_vec(), b"value2".to_vec()),
            (b"key3".to_vec(), b"value3".to_vec()),
        ];
        
        trie.batch_insert(items.clone()).unwrap();
        
        // 验证所有项目
        for (key, value) in &items {
            assert_eq!(trie.get(key).unwrap().unwrap(), *value);
        }
        
        // 批量删除部分项目
        let keys_to_delete = vec![b"key1".to_vec(), b"key3".to_vec()];
        trie.batch_remove(keys_to_delete).unwrap();
        
        // 验证删除结果
        assert!(trie.get(b"key1").unwrap().is_none());
        assert_eq!(trie.get(b"key2").unwrap().unwrap(), b"value2");
        assert!(trie.get(b"key3").unwrap().is_none());
    }

    #[test]
    fn test_empty_tree_operations() {
        let kv = Arc::new(kvdb_memorydb::create(1));
        let mut trie = AssetTrie::<Layout>::new(kv, Default::default());

        // 在空树上删除应该成功但无效果
        assert!(trie.remove(b"nonexistent").is_ok());
        
        // 空树应该返回 None
        assert!(trie.get(b"anything").unwrap().is_none());
        assert!(!trie.contains(b"anything").unwrap());
        
        // 空树的 iter_all 应该返回空 HashMap
        assert!(trie.iter_all().unwrap().is_empty());
    }

    #[test]
    fn test_disk_persistence() {
        let node_dir = Path::new("./testdata/improved_test");
        
        if node_dir.exists() {
            let _ = fs::remove_dir_all(&node_dir);
        }
        fs::create_dir_all(&node_dir).expect("创建测试目录失败");

        let config = DatabaseConfig::with_columns(1);
        
        let saved_root = {
            // 在这个作用域内创建和使用第一个数据库实例
            let db = Arc::new(RocksDb::open(&config, &node_dir).expect("打开数据库失败"));
            let mut trie = AssetTrie::<Layout>::new(db.clone(), Default::default());
            
            // 插入测试数据
            let items = vec![
                (b"persistent_1".to_vec(), b"data_1".to_vec()),
                (b"persistent_2".to_vec(), b"data_2".to_vec()),
            ];
            
            trie.batch_insert(items).unwrap();
            let root = trie.root();
            
            // 显式删除数据库引用，确保锁被释放
            drop(db);
            
            root
        }; // 第一个数据库实例在这里被完全释放
        
        // 等一小段时间确保锁被完全释放
        std::thread::sleep(std::time::Duration::from_millis(100));
        
        // 重新打开数据库，使用保存的根
        let db2 = Arc::new(RocksDb::open(&config, &node_dir).expect("重新打开数据库失败"));
        let trie2 = AssetTrie::<Layout>::new(db2, saved_root);
        
        // 验证数据持久化
        assert_eq!(trie2.get(b"persistent_1").unwrap().unwrap(), b"data_1");
        assert_eq!(trie2.get(b"persistent_2").unwrap().unwrap(), b"data_2");
        
        // 清理
        let _ = fs::remove_dir_all(&node_dir);
    }
    #[test]
    fn test_historical_roots() {
        let kv = Arc::new(kvdb_memorydb::create(1));
        let mut trie = AssetTrie::<Layout>::new(kv.clone(), Default::default());

        // 步骤1：创建 root1，包含 key1:value1, key2:value2
        let items1 = vec![
            (b"key1".to_vec(), b"value1".to_vec()),
            (b"key2".to_vec(), b"value2".to_vec()),
        ];
        
        trie.batch_insert(items1).unwrap();
        let root1 = trie.root();
        println!("Root1: {:?}", root1);
        
        // 验证 root1 的状态
        assert_eq!(trie.get(b"key1").unwrap().unwrap(), b"value1");
        assert_eq!(trie.get(b"key2").unwrap().unwrap(), b"value2");
        
        // 步骤2：删除 key1，修改 key2 为 new_value2，得到 root2
        trie.remove(b"key1").unwrap();
        trie.insert(b"key2", b"new_value2").unwrap();
        let root2 = trie.root();
        println!("Root2: {:?}", root2);
        
        // 验证根哈希确实发生了变化
        assert_ne!(root1, root2, "Root hashes should be different");
        
        // 步骤3：测试历史根功能
        
        // 使用 root2（当前状态）验证
        let trie_root2 = AssetTrie::<Layout>::new(kv.clone(), root2);
        assert!(trie_root2.get(b"key1").unwrap().is_none(), "key1 should not exist in root2");
        assert_eq!(trie_root2.get(b"key2").unwrap().unwrap(), b"new_value2", "key2 should have new_value2 in root2");
        
        // 使用 root1（历史状态）验证
        let trie_root1 = AssetTrie::<Layout>::new(kv.clone(), root1);
        assert_eq!(trie_root1.get(b"key1").unwrap().unwrap(), b"value1", "key1 should exist with value1 in root1");
        assert_eq!(trie_root1.get(b"key2").unwrap().unwrap(), b"value2", "key2 should have value2 in root1");
        
        println!("Historical roots test passed!");
        println!("Root1 preserves: key1=value1, key2=value2");
        println!("Root2 contains: key2=new_value2 (key1 deleted)");
    }
}