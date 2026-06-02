use crate::errors::StorageError;
use dashmap::DashMap;
use heed;
use heed::byteorder::BigEndian;
use heed::types::{Bytes, U128};
use heed::{Database, Env, EnvOpenOptions, WithoutTls};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct StorageBackend {
    pub env: Arc<Env<WithoutTls>>,
    dbs: Arc<DashMap<String, Database<U128<BigEndian>, Bytes>>>,
    dbi_open_lock: Arc<Mutex<()>>,
}

impl From<heed::Error> for StorageError {
    fn from(err: heed::Error) -> Self {
        match err {
            heed::Error::Io(e) => StorageError::GenericIoError(e),
            heed::Error::Encoding(e) => StorageError::WriteError(e.to_string()),
            heed::Error::Decoding(e) => StorageError::ReadError(e.to_string()),
            _ => StorageError::DatabaseError(err.to_string()),
        }
    }
}

impl StorageBackend {
    pub fn initialize(store_path: Option<PathBuf>, map_size: usize) -> Result<Self, StorageError>
    where
        Self: Sized,
    {
        let Some(checked_path) = store_path else {
            return Err(StorageError::InvalidPath);
        };
        if !checked_path.exists() {
            std::fs::create_dir_all(&checked_path)?;
        }
        let rounded_map_size = ((map_size as f64 / page_size::get() as f64).round()
            * page_size::get() as f64) as usize;
        unsafe {
            let backend = StorageBackend {
                env: Arc::new(
                    EnvOpenOptions::new()
                        .read_txn_without_tls()
                        // Change this as more tables are needed.
                        .max_dbs(3)
                        .map_size(rounded_map_size)
                        .open(checked_path)
                        .map_err(|e| StorageError::DatabaseInitError(e.to_string()))?,
                ),
                dbs: Arc::new(DashMap::new()),
                dbi_open_lock: Arc::new(Mutex::new(())),
            };
            Ok(backend)
        }
    }

    pub fn insert(&self, table: &str, key: u128, value: Vec<u8>) -> Result<(), StorageError> {
        let mut rw_txn = self.env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = self.get_db(table)?;
        if db.get_or_put(&mut rw_txn, &key, &value)?.is_some() {
            return Err(StorageError::KeyExists(key as u64));
        }
        rw_txn.commit()?;
        Ok(())
    }

    pub fn get(&self, table: &str, key: u128) -> Result<Option<Vec<u8>>, StorageError> {
        let ro_txn = self.env.read_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = self.get_db(table)?;
        Ok(db.get(&ro_txn, &key)?.map(|v| v.to_vec()))
    }

    pub fn delete(&self, table: &str, key: u128) -> Result<(), StorageError> {
        let mut rw_txn = self.env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = self.get_db(table)?;
        if !db.delete(&mut rw_txn, &key)? {
            return Err(StorageError::KeyNotFound(key as u64));
        }
        rw_txn.commit()?;
        Ok(())
    }

    pub fn update(&self, table: &str, key: u128, value: Vec<u8>) -> Result<(), StorageError> {
        let mut rw_txn = self.env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = self.get_db(table)?;
        if db.get(&rw_txn, &key)?.is_none() {
            return Err(StorageError::KeyNotFound(key as u64));
        }
        db.put(&mut rw_txn, &key, &value)?;
        rw_txn.commit()?;
        Ok(())
    }

    pub fn upsert(&self, table: &str, key: u128, value: Vec<u8>) -> Result<bool, StorageError> {
        let mut rw_txn = self.env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = self.get_db(table)?;
        db.put(&mut rw_txn, &key, &value)?;
        rw_txn.commit()?;
        Ok(true)
    }

    pub fn exists(&self, table: &str, key: u128) -> Result<bool, StorageError> {
        let ro_txn = self.env.read_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = self.get_db(table)?;
        Ok(db.get(&ro_txn, &key)?.is_some())
    }

    pub fn table_exists(&self, table: &str) -> Result<bool, StorageError> {
        if self.dbs.contains_key(table) {
            return Ok(true);
        }
        let _lock = self.dbi_open_lock.lock();
        if self.dbs.contains_key(table) {
            return Ok(true);
        }
        let rw_txn = self.env.write_txn()?;
        match self
            .env
            .open_database::<U128<BigEndian>, Bytes>(&rw_txn, Some(table))?
        {
            Some(db) => {
                rw_txn.commit()?;
                self.dbs.insert(table.to_string(), db);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn details(&self) -> String {
        format!("LMDB (heed 0.22.1): {:?}", self.env.info())
    }

    pub fn flush(&self) -> Result<(), StorageError> {
        self.env.clear_stale_readers()?;
        self.env.force_sync()?;
        Ok(())
    }

    pub fn create_table(&self, table: &str) -> Result<(), StorageError> {
        let mut rw_txn = self.env.write_txn()?;
        let db = self
            .env
            .create_database::<U128<BigEndian>, Bytes>(&mut rw_txn, Some(table))?;
        rw_txn.commit()?;
        self.dbs.insert(table.to_string(), db);
        Ok(())
    }

    pub fn close(&self) -> Result<(), StorageError> {
        self.flush()?;
        Ok(())
    }

    pub fn get_env(&self) -> Arc<Env<WithoutTls>> {
        self.env.clone()
    }

    fn get_db(&self, table: &str) -> Result<Database<U128<BigEndian>, Bytes>, StorageError> {
        // Already in cache
        if let Some(db) = self.dbs.get(table) {
            return Ok(*db);
        }
        // First time
        let _lock = self.dbi_open_lock.lock();
        // Double-check
        if let Some(db) = self.dbs.get(table) {
            return Ok(*db);
        }
        let rw_txn = self.env.write_txn()?;
        let db = self
            .env
            .open_database::<U128<BigEndian>, Bytes>(&rw_txn, Some(table))?
            .ok_or_else(|| StorageError::TableError("Table not found".to_string()))?;
        rw_txn.commit()?;
        self.dbs.insert(table.to_string(), db);
        Ok(db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::remove_dir_all;
    use std::hash::Hasher;
    use tempfile::tempdir;

    fn hash_2_to_u128(a: u64, b: u64) -> u128 {
        let mut hasher = wyhash::WyHash::with_seed(0);
        hasher.write_u64(a);
        hasher.write_u64(b);
        u128::from(hasher.finish())
    }

    #[test]
    fn test_write() {
        let path = tempdir().unwrap().keep();
        {
            let backend =
                StorageBackend::initialize(Some(path.clone()), 10 * 1024 * 1024 * 1024).unwrap();
            backend.create_table("test_table").unwrap();
            let key = 12345678901234567890u128;
            let value = vec![1, 2, 3, 4, 5];
            backend.insert("test_table", key, value.clone()).unwrap();
            let retrieved_value = backend.get("test_table", key).unwrap();
            assert_eq!(retrieved_value, Some(value));
        }
        remove_dir_all(path).unwrap();
    }

    #[test]
    fn test_concurrent_write() {
        let path = tempdir().unwrap().keep();
        {
            let backend =
                StorageBackend::initialize(Some(path.clone()), 10 * 1024 * 1024 * 1024).unwrap();
            backend.create_table("test_table").unwrap();
            let mut threads = vec![];
            for thread_iter in 0..10 {
                let handle = std::thread::spawn({
                    let backend = backend.clone();
                    move || {
                        for iter in 0..100 {
                            let key = hash_2_to_u128(iter, thread_iter);
                            let value = vec![rand::random::<u8>(); 10];
                            backend.insert("test_table", key, value).unwrap();
                        }
                    }
                });
                threads.push(handle);
            }
            for handle in threads {
                handle.join().unwrap();
            }
        }
        remove_dir_all(path).unwrap();
    }

    #[test]
    fn test_concurrent_read() {
        let path = tempdir().unwrap().keep();
        {
            let backend =
                StorageBackend::initialize(Some(path.clone()), 10 * 1024 * 1024 * 1024).unwrap();
            backend.create_table("test_table").unwrap();
            for thread_iter in 0..10 {
                for iter in 0..100 {
                    let value = vec![rand::random::<u8>(); 10];
                    let key = hash_2_to_u128(iter, thread_iter);
                    backend.insert("test_table", key, value).unwrap();
                }
            }
            let mut threads = vec![];
            for thread_iter in 0..10 {
                let handle = std::thread::spawn({
                    let backend = backend.clone();
                    move || {
                        for iter in 0..100 {
                            let key = hash_2_to_u128(iter, thread_iter);
                            let _ = backend.get("test_table", key).unwrap();
                        }
                    }
                });
                threads.push(handle);
            }
            for handle in threads {
                handle.join().unwrap();
            }
        }
        remove_dir_all(path).unwrap();
    }
}
