use std::hash::{Hash, Hasher};
use temper_core::dimension::Dimension;
use temper_core::pos::ChunkPos;
use temper_storage::lmdb::StorageBackend;
use temper_world_format::Chunk;
use temper_world_format::errors::WorldError;
use temper_world_format::errors::WorldError::CorruptedChunkData;
use tracing::warn;
use yazi::CompressionLevel;

pub fn save_chunk_internal(
    storage: &StorageBackend,
    pos: ChunkPos,
    dimension: Dimension,
    chunk: &Chunk,
) -> Result<(), WorldError> {
    if !storage.table_exists("chunks")? {
        storage.create_table("chunks")?;
    }
    let as_bytes = yazi::compress(
        &bitcode::serialize(chunk).expect("Unable to serialize chunk"),
        yazi::Format::Zlib,
        CompressionLevel::BestSpeed,
    )?;
    let digest = create_key(dimension, pos);
    storage.upsert("chunks", digest, as_bytes)?;
    Ok(())
}

pub fn load_chunk_internal(
    storage: &StorageBackend,
    pos: ChunkPos,
    dimension: Dimension,
    verify: bool,
) -> Result<Chunk, WorldError> {
    let digest = create_key(dimension, pos);
    match storage.get("chunks", digest)? {
        Some(compressed) => {
            let (data, checksum) = yazi::decompress(compressed.as_slice(), yazi::Format::Zlib)?;
            if verify {
                if let Some(expected_checksum) = checksum {
                    let real_checksum = yazi::Adler32::from_buf(data.as_slice()).finish();
                    if real_checksum != expected_checksum {
                        return Err(CorruptedChunkData(real_checksum, expected_checksum));
                    }
                } else {
                    warn!("Chunk data does not have a checksum, skipping verification.");
                }
            }
            let chunk: Chunk = bitcode::deserialize(&data)
                .map_err(|e| WorldError::BitcodeDeserializeError(e.to_string()))?;
            Ok(chunk)
        }
        None => Err(WorldError::ChunkNotFound),
    }
}

pub fn chunk_exists_internal(
    storage: &StorageBackend,
    pos: ChunkPos,
    dimension: Dimension,
) -> Result<bool, WorldError> {
    if !storage.table_exists("chunks")? {
        return Ok(false);
    }
    let digest = create_key(dimension, pos);
    Ok(storage.exists("chunks", digest)?)
}

pub fn delete_chunk_internal(
    storage: &StorageBackend,
    pos: ChunkPos,
    dimension: Dimension,
) -> Result<(), WorldError> {
    let digest = create_key(dimension, pos);
    storage.delete("chunks", digest)?;
    Ok(())
}

pub fn sync_internal(storage: &StorageBackend) -> Result<(), WorldError> {
    storage.flush()?;
    Ok(())
}

fn create_key(dimension: Dimension, pos: ChunkPos) -> u128 {
    let mut hasher = wyhash::WyHash::with_seed(0);
    dimension.hash(&mut hasher);
    let dim_hash = hasher.finish();
    u128::from(dim_hash) << 96 | u128::from(pos.pack())
}
