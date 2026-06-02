use std::collections::HashSet;
use std::sync::Arc;
use temper_storage::lmdb::StorageBackend;

fn generate_random_data(size: usize) -> Vec<u8> {
    (0..size).map(|_| rand::random::<u8>()).collect()
}

fn generate_random_key(used: &mut HashSet<u128>) -> u128 {
    let mut key = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    while used.contains(&key) {
        key += 1;
    }
    used.insert(key);
    key
}

fn select_random<T: Clone + Copy>(choices: &[T]) -> T {
    let index = rand::random::<u32>() as usize % choices.len();
    choices[index]
}

pub(crate) fn db_benches(c: &mut criterion::Criterion) {
    let mut used_keys = HashSet::new();
    let tempdir = tempfile::TempDir::new().unwrap().keep();

    let db = StorageBackend::initialize(Some(tempdir), 100 * 1024 * 1024 * 1024).unwrap();

    db.create_table("insert_test").unwrap();

    let mut insert_group = c.benchmark_group("Insert");

    insert_group.bench_function("512b", |b| {
        b.iter(|| {
            db.insert(
                "insert_test",
                generate_random_key(&mut used_keys),
                generate_random_data(512),
            )
            .unwrap();
        })
    });

    insert_group.bench_function("1kb", |b| {
        b.iter(|| {
            db.insert(
                "insert_test",
                generate_random_key(&mut used_keys),
                generate_random_data(1024),
            )
            .unwrap();
        })
    });

    insert_group.bench_function("4kb", |b| {
        b.iter(|| {
            db.insert(
                "insert_test",
                generate_random_key(&mut used_keys),
                generate_random_data(4096),
            )
            .unwrap();
        })
    });

    insert_group.finish();

    let mut read_group = c.benchmark_group("Read");

    db.create_table("read_test").unwrap();

    let keys_512b: Vec<u128> = (0..1000)
        .map(|_| generate_random_key(&mut used_keys))
        .collect();
    for key in keys_512b.iter() {
        db.insert("read_test", *key, generate_random_data(512))
            .unwrap();
    }
    read_group.bench_function("512b", |b| {
        b.iter(|| db.get("read_test", select_random(&keys_512b)).unwrap())
    });

    let keys_1kb: Vec<u128> = (0..1000)
        .map(|_| generate_random_key(&mut used_keys))
        .collect();
    for key in keys_1kb.iter() {
        db.insert("read_test", *key, generate_random_data(1024))
            .unwrap();
    }
    read_group.bench_function("1kb", |b| {
        b.iter(|| db.get("read_test", select_random(&keys_1kb)).unwrap())
    });

    let keys_4kb: Vec<u128> = (0..1000)
        .map(|_| generate_random_key(&mut used_keys))
        .collect();
    for key in keys_4kb.iter() {
        db.insert("read_test", *key, generate_random_data(4096))
            .unwrap();
    }
    read_group.bench_function("4kb", |b| {
        b.iter(|| db.get("read_test", select_random(&keys_4kb)).unwrap())
    });

    read_group.finish();

    let mut concurrent_group = c.benchmark_group("ConcurrentRead");

    db.create_table("concurrent_test").unwrap();

    let concurrent_keys: Vec<u128> = (0..1000)
        .map(|_| generate_random_key(&mut used_keys))
        .collect();
    for key in concurrent_keys.iter() {
        db.insert("concurrent_test", *key, generate_random_data(512))
            .unwrap();
    }

    let db = Arc::new(db);

    for thread_count in [2usize, 4, 8, 16] {
        let db = Arc::clone(&db);
        let keys = concurrent_keys.clone();

        concurrent_group.bench_function(format!("{thread_count}_threads_512b"), |b| {
            b.iter(|| {
                let handles: Vec<_> = (0..thread_count)
                    .map(|_| {
                        let db = Arc::clone(&db);
                        let keys = keys.clone();
                        std::thread::spawn(move || {
                            db.get("concurrent_test", select_random(&keys)).unwrap();
                        })
                    })
                    .collect();
                for h in handles {
                    h.join().unwrap();
                }
            })
        });
    }

    concurrent_group.finish();
}
