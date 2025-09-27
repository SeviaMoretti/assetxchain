// patricia_trie_mpt.rs     
// IncompleteDatabase([188, 54, 120, 158, 122, 30, 40, 20, 54, 70, 66, 41, 130, 143, 129, 125, 102, 18, 247, 180, 119, 214, 101, 145, 255, 150, 169, 224, 100, 188, 201, 138])
use std::marker::PhantomData;
use std::error::Error;

use kvdb::KeyValueDB;
use hash_db::{Hasher, HashDB};
use trie_db::{DBValue, Trie, TrieHash, TrieLayout, TrieDBMutBuilder, TrieMut, TrieDBBuilder};
// use codec::{Encode, Decode};

use crate::kvdb_hashdb::KvdbHashDB;

/// KVDB-backed AssetTrie
pub struct AssetTrie<'a, L: TrieLayout>
where
    L::Hash: Hasher + 'static,
{
    kv: &'a dyn KeyValueDB,
    root: TrieHash<L>,
    _marker: PhantomData<L>,
}

// 需要处理树为空（树根设为Dafult::default()）的情况
impl<'a, L> AssetTrie<'a, L>
where
    L: TrieLayout + 'static,
    L::Hash: Hasher + 'static,
    // make sure associated types used by TrieDBMut have 'static lifetime
    <<L as TrieLayout>::Hash as Hasher>::Out: 'static,
    // codec error type (if codec exists) should be 'static as well; this is conservative
    // Note: trie_db's Codec trait path may differ depending on version; keeping as generic bound
    // ensures associated types outlive the function scope.
    // If you still get compiler complaints about Codec::Error, refine this bound to the exact path.
{
    /// 创建 AssetTrie，initial_root 必须是 TrieHash<L>
    pub fn new(kv: &'a dyn KeyValueDB, initial_root: TrieHash<L>) -> Self {
        Self { kv, root: initial_root, _marker: PhantomData }
    }

    /// 返回当前 root（TrieHash<L>）
    pub fn root(&self) -> TrieHash<L> {
        self.root.clone()
    }

    /// 单条插入（trie 对象限定在局部作用域以便正确 drop）
    pub fn insert(&mut self, key: &[u8], value: &[u8]) -> Result<TrieHash<L>, Box<dyn Error>> {
        let mut hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        let mut root_local: TrieHash<L> = self.root.clone();
        {
            // 创建并使用 trie 的局部作用域，确保 trie 在离开此块时被 drop
            let mut trie = if root_local == Default::default() {
                TrieDBMutBuilder::<L>::new(&mut hashdb, &mut root_local).build()
            } else {
                TrieDBMutBuilder::<L>::from_existing(&mut hashdb, &mut root_local).build()
            };
            // let mut trie = builder.build();

            trie.insert(key, value)?;
            // 当作用域结束时，trie 被 drop(commit)，任何对 root_local 的可变借用也随之结束
        }

        // 现在安全地更新 self.root
        self.root = root_local;
        Ok(self.root.clone())
    }

    /// 批量插入（在一个 TrieDBMut 上多次 insert，提高效率）
    pub fn batch_insert<I>(&mut self, items: I) -> Result<TrieHash<L>, Box<dyn Error>>
    where
        I: IntoIterator<Item = (Vec<u8>, Vec<u8>)>,
    {
        let mut hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        let mut root_local: TrieHash<L> = self.root.clone();

        {
            let mut trie = if root_local == Default::default() {
                TrieDBMutBuilder::<L>::new(&mut hashdb, &mut root_local).build()
            } else {
                TrieDBMutBuilder::<L>::from_existing(&mut hashdb, &mut root_local).build()
            };

            for (k, v) in items {
                trie.insert(&k, &v)?;
            }
            // trie dropped here
        }

        self.root = root_local;
        Ok(self.root.clone())
    }

    /// 单条删除
    pub fn remove(&mut self, key: &[u8]) -> Result<TrieHash<L>, Box<dyn Error>> {
        println!("enter remove key: {:?}\n", key);
        let mut hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        let mut root_local: TrieHash<L> = self.root.clone();

        {
            let mut trie = if root_local == Default::default() {
                TrieDBMutBuilder::<L>::new(&mut hashdb, &mut root_local).build()
            } else {
                TrieDBMutBuilder::<L>::from_existing(&mut hashdb, &mut root_local).build()
            };
            trie.remove(key)?;
            // trie dropped here
        }

        self.root = root_local;
        Ok(self.root.clone())
    }

    /// 批量删除
    pub fn batch_remove<I>(&mut self, keys: I) -> Result<TrieHash<L>, Box<dyn Error>>
    where
        I: IntoIterator<Item = Vec<u8>>,
    {
        let mut hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        let mut root_local: TrieHash<L> = self.root.clone();

        {
            let mut trie = if root_local == Default::default() {
                TrieDBMutBuilder::<L>::new(&mut hashdb, &mut root_local).build()
            } else {
                TrieDBMutBuilder::<L>::from_existing(&mut hashdb, &mut root_local).build()
            };
            for k in keys {
                trie.remove(&k)?;
            }
            // trie dropped here
        }

        self.root = root_local;
        Ok(self.root.clone())
    }

    /// 只读 get（同样放在局部作用域）
    pub fn get(&self, key: &[u8]) -> Result<Option<DBValue>, Box<dyn Error>> {
        println!("enter get key: {:?}\n", key);
        // get为空是不需要new trie，直接返回None
        if self.root == Default::default() {
            return Ok(None);
        }

        let hashdb = KvdbHashDB::<L::Hash>::new(self.kv);
        let root_local: TrieHash<L> = self.root.clone();

        let result = {
            let trie = TrieDBBuilder::<L>::new(&hashdb, &root_local).build();
            // 注意：trie.get 返回 Result<Option<&[u8]>, _>
            match trie.get(key) {
                Ok(opt) => Ok(opt.map(|v| v.to_vec())),
                Err(e) => Err(Box::new(e) as Box<dyn Error>),
            }
            // trie dropped at end of this inner block
        };

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    // RocksDB-backed kvdb (kvdb-rocksdb)
    use kvdb_rocksdb::{Database as RocksDb, DatabaseConfig};

    use reference_trie::NoExtensionLayout as Layout;

    use trie_db::TrieHash;

    #[test]
    fn test_asset_trie_disk_basic_ops() {
        // 项目根目录下的 node 文件夹
        let node_dir = Path::new("./testdata/diskt");

        // 清理之前的测试数据（若存在）
        if node_dir.exists() {
            let _ = fs::remove_dir_all(&node_dir);
        }
        fs::create_dir_all(&node_dir).expect("create node dir failed");

        // kvdb-rocksdb 最新 API: 先创建 DatabaseConfig，然后 open(&config, path).
        // （注意：不同版本的 kvdb-rocksdb open 可能不同。）
        let config = DatabaseConfig::with_columns(1); // 1 列就够做测试
        // 可以按需调整 config.memory_budget / max_open_files 等
        let db = RocksDb::open(&config, &node_dir).expect("open rocksdb failed");

        // initial root：使用默认空 root
        let initial_root: TrieHash<Layout> = Default::default();

        // 创建 AssetTrie（使用文件里的构造函数）
        // AssetTrie::new 的签名是 fn new(kv: &'a dyn KeyValueDB, initial_root: TrieHash<L>) -> Self
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
}