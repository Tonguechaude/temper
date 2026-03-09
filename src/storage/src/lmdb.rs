use crate::errors::StorageError;
use heed;
use heed::byteorder::BigEndian;
use heed::types::{Bytes, U128};
use heed::{Database, Env, EnvOpenOptions, WithoutTls};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct LmdbBackend {
    pub env: Arc<Mutex<Env<WithoutTls>>>,
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

impl LmdbBackend {
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
            let backend = LmdbBackend {
                env: Arc::new(Mutex::new(
                    EnvOpenOptions::new()
                        .read_txn_without_tls()
                        // Change this as more tables are needed.
                        .max_dbs(3)
                        .map_size(rounded_map_size)
                        .open(checked_path)
                        .map_err(|e| StorageError::DatabaseInitError(e.to_string()))?,
                )),
            };
            Ok(backend)
        }
    }

    pub fn insert(&self, table: String, key: u128, value: Vec<u8>) -> Result<(), StorageError> {
        let env = self.env.lock();
        let mut rw_txn = env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> =
            env.create_database(&mut rw_txn, Some(&table))?;
        if db.get(&rw_txn, &key)?.is_some() {
            return Err(StorageError::KeyExists(key as u64));
        }
        db.put(&mut rw_txn, &key, &value)?;
        rw_txn.commit()?;
        Ok(())
    }

    pub fn get(&self, table: String, key: u128) -> Result<Option<Vec<u8>>, StorageError> {
        let env = self.env.lock();
        let ro_txn = env.read_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = env
            .open_database(&ro_txn, Some(&table))?
            .ok_or(StorageError::TableError("Table not found".to_string()))?;
        let value = db.get(&ro_txn, &key)?;
        if let Some(v) = value {
            Ok(Some(v.to_vec()))
        } else {
            Ok(None)
        }
    }

    pub fn delete(&self, table: String, key: u128) -> Result<(), StorageError> {
        let env = self.env.lock();
        let mut rw_txn = env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = env
            .open_database(&rw_txn, Some(&table))?
            .ok_or(StorageError::TableError("Table not found".to_string()))?;
        if db.get(&rw_txn, &key)?.is_none() {
            return Err(StorageError::KeyNotFound(key as u64));
        }
        db.delete(&mut rw_txn, &key)?;
        rw_txn.commit()?;
        Ok(())
    }

    pub fn update(&self, table: String, key: u128, value: Vec<u8>) -> Result<(), StorageError> {
        let env = self.env.lock();
        let mut rw_txn = env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = env
            .open_database(&rw_txn, Some(&table))?
            .ok_or(StorageError::TableError("Table not found".to_string()))?;
        if db.get(&rw_txn, &key)?.is_none() {
            return Err(StorageError::KeyNotFound(key as u64));
        }
        db.put(&mut rw_txn, &key, &value)?;
        rw_txn.commit()?;
        Ok(())
    }

    pub fn upsert(&self, table: String, key: u128, value: Vec<u8>) -> Result<bool, StorageError> {
        let env = self.env.lock();
        let mut rw_txn = env.write_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = env
            .open_database(&rw_txn, Some(&table))?
            .ok_or(StorageError::TableError("Table not found".to_string()))?;
        db.put(&mut rw_txn, &key, &value)?;
        rw_txn.commit()?;
        Ok(true)
    }

    pub fn exists(&self, table: String, key: u128) -> Result<bool, StorageError> {
        let env = self.env.lock();
        let ro_txn = env.read_txn()?;
        let db: Database<U128<BigEndian>, Bytes> = env
            .open_database(&ro_txn, Some(&table))?
            .ok_or(StorageError::TableError("Table not found".to_string()))?;
        Ok(db.get(&ro_txn, &key)?.is_some())
    }

    pub fn table_exists(&self, table: String) -> Result<bool, StorageError> {
        let env = self.env.lock();
        let ro_txn = env.read_txn()?;
        let db = env.open_database::<U128<BigEndian>, Bytes>(&ro_txn, Some(&table))?;
        Ok(db.is_some())
    }

    pub fn details(&self) -> String {
        format!("LMDB (heed 0.20.5): {:?}", self.env.lock().info())
    }

    pub fn flush(&self) -> Result<(), StorageError> {
        let env = self.env.lock();
        env.clear_stale_readers()?;
        env.force_sync()?;
        Ok(())
    }

    pub fn create_table(&self, table: String) -> Result<(), StorageError> {
        let env = self.env.lock();
        let mut rw_txn = env.write_txn()?;
        env.create_database::<U128<BigEndian>, Bytes>(&mut rw_txn, Some(&table))?;
        rw_txn.commit()?;
        Ok(())
    }

    pub fn close(&self) -> Result<(), StorageError> {
        self.flush()?;
        Ok(())
    }

    pub fn get_env(&self) -> Arc<Mutex<Env<WithoutTls>>> {
        self.env.clone()
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
        hasher.finish() as u128
    }

    #[test]
    fn test_write() {
        let path = tempdir().unwrap().keep();
        {
            let backend =
                LmdbBackend::initialize(Some(path.clone()), 10 * 1024 * 1024 * 1024).unwrap();
            backend.create_table("test_table".to_string()).unwrap();
            let key = 12345678901234567890u128;
            let value = vec![1, 2, 3, 4, 5];
            backend
                .insert("test_table".to_string(), key, value.clone())
                .unwrap();
            let retrieved_value = backend.get("test_table".to_string(), key).unwrap();
            assert_eq!(retrieved_value, Some(value));
        }
        remove_dir_all(path).unwrap();
    }

    #[test]
    fn test_concurrent_write() {
        let path = tempdir().unwrap().keep();
        {
            let backend =
                LmdbBackend::initialize(Some(path.clone()), 10 * 1024 * 1024 * 1024).unwrap();
            backend.create_table("test_table".to_string()).unwrap();
            let mut threads = vec![];
            for thread_iter in 0..10 {
                let handle = std::thread::spawn({
                    let backend = backend.clone();
                    move || {
                        for iter in 0..100 {
                            let key = hash_2_to_u128(iter, thread_iter);
                            let value = vec![rand::random::<u8>(); 10];
                            backend
                                .insert("test_table".to_string(), key, value)
                                .unwrap();
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
                LmdbBackend::initialize(Some(path.clone()), 10 * 1024 * 1024 * 1024).unwrap();
            backend.create_table("test_table".to_string()).unwrap();
            for thread_iter in 0..10 {
                for iter in 0..100 {
                    let value = vec![rand::random::<u8>(); 10];
                    let key = hash_2_to_u128(iter, thread_iter);
                    backend
                        .insert("test_table".to_string(), key, value)
                        .unwrap();
                }
            }
            let mut threads = vec![];
            for thread_iter in 0..10 {
                let handle = std::thread::spawn({
                    let backend = backend.clone();
                    move || {
                        for iter in 0..100 {
                            let key = hash_2_to_u128(iter, thread_iter);
                            let _ = backend.get("test_table".to_string(), key).unwrap();
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
