//! Launch utilities for server initialization, chunk generation, and world import.

use crate::errors::BinaryError;
use std::time::Instant;
use temper_components::player::offline_player_data::OfflinePlayerData;
use temper_core::block_state_id::{
    BLOCK2ID, ID2BLOCK, ITEM_TO_BLOCK_MAPPING, create_block_mappings, create_item_to_block_mapping,
};
use temper_core::dimension::Dimension;
use temper_core::pos::ChunkPos;
use temper_state::GlobalState;
use temper_storage::string_to_u128;
use temper_world_format::Chunk;
use tracing::{error, info};
use type_hash::TypeHash;

/// Generates spawn chunks around the origin if they don't exist.
pub fn generate_spawn_chunks(state: GlobalState) -> Result<(), BinaryError> {
    info!("No overworld spawn chunk found, generating spawn chunks...");

    let start = Instant::now();
    let radius = state.config.chunk_render_distance as i32;

    // Collect all chunk coordinates to generate
    let chunks: Vec<(i32, i32)> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |z| (x, z)))
        .collect();

    let mut batch = state.thread_pool.batch();
    for (x, z) in chunks {
        let state_clone = state.clone();
        batch.execute(move || {
            let pos = ChunkPos::new(x, z);
            let chunk = state_clone.world.world_generator.generate_chunk(pos);

            match chunk {
                Ok(chunk) => {
                    if let Err(e) = state_clone
                        .world
                        .insert_chunk(pos, Dimension::Overworld, chunk)
                    {
                        error!("Error saving chunk ({}, {}): {:?}", x, z, e);
                    }
                }
                Err(e) => {
                    error!("Error generating chunk ({}, {}): {:?}", x, z, e);
                }
            }
        });
    }
    batch.wait();

    info!("Finished generating spawn chunks in {:?}", start.elapsed());
    Ok(())
}

pub fn setup_db(state: GlobalState) -> Result<(), BinaryError> {
    info!("Setting up database...");

    state.world.storage_backend.create_table("metadata")?;

    let chunk_key = string_to_u128("chunk-format-hash");
    state.world.storage_backend.upsert(
        "metadata",
        chunk_key,
        Chunk::type_hash().to_be_bytes().to_vec(),
    )?;

    let player_key = string_to_u128("player-format-hash");
    state.world.storage_backend.upsert(
        "metadata",
        player_key,
        OfflinePlayerData::type_hash().to_be_bytes().to_vec(),
    )?;

    info!("Database setup complete.");
    Ok(())
}

pub fn setup_block_and_item_mapping() {
    ITEM_TO_BLOCK_MAPPING
        .set(create_item_to_block_mapping())
        .ok();
    let (id2block, block2id) = create_block_mappings();
    ID2BLOCK.set(id2block).ok();
    BLOCK2ID.set(block2id).ok();
}
