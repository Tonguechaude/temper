use crate::errors::BlockPlaceError;
use crate::{BlockFace, BlockStateId};
use crate::{BlockPlaceContext, PlacableBlock, PlacedBlocks};
use bevy_math::IVec3;
use std::collections::BTreeMap;
use temper_core::block_data::BlockData;
use temper_core::dimension::Dimension;
use temper_macros::{item, match_block};
use temper_state::GlobalState;
use tracing::error;

pub(crate) struct PlaceableDoor;

impl PlacableBlock for PlaceableDoor {
    fn place(
        context: BlockPlaceContext,
        state: GlobalState,
    ) -> Result<PlacedBlocks, BlockPlaceError> {
        let name = match context.item_used {
            item!("oak_door") => "minecraft:oak_door",
            item!("birch_door") => "minecraft:birch_door",
            item!("spruce_door") => "minecraft:spruce_door",
            item!("jungle_door") => "minecraft:jungle_door",
            item!("acacia_door") => "minecraft:acacia_door",
            item!("dark_oak_door") => "minecraft:dark_oak_door",
            _ => return Err(BlockPlaceError::ItemNotMappedToBlock(context.item_used)),
        };
        let block_above = {
            let chunk = state
                .world
                .get_or_generate_chunk(context.block_position.chunk(), Dimension::Overworld)
                .expect("Could not load chunk");
            chunk.get_block((context.block_position.pos + IVec3::new(0, 1, 0)).into())
        };
        if !(match_block!("air", block_above) || match_block!("cave_air", block_above)) {
            return Ok(PlacedBlocks {
                blocks: std::collections::HashMap::new(),
                take_item: false,
            });
        };
        let facing = match context.face_clicked {
            BlockFace::North => "south",
            BlockFace::South => "north",
            BlockFace::East => "west",
            BlockFace::West => "east",
            BlockFace::Top => {
                // Facing is determined by player rotation when placing on top face
                let yaw = (context.player_rotation.yaw + 180.0) % 360.0;
                if (45.0..135.0).contains(&yaw) {
                    "east"
                } else if (135.0..225.0).contains(&yaw) {
                    "south"
                } else if (225.0..315.0).contains(&yaw) {
                    "west"
                } else {
                    "north"
                }
            }
            _ => return Err(BlockPlaceError::InvalidBlockFace(context.face_clicked)),
        };
        let bottom_block = BlockData {
            name: name.to_string(),
            properties: Some(BTreeMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "lower".to_string()),
                ("hinge".to_string(), "left".to_string()),
                ("open".to_string(), "false".to_string()),
                ("powered".to_string(), "false".to_string()),
            ])),
        };
        let Some(bottom_block_id) = bottom_block.try_to_block_state_id() else {
            error!("Block data '{bottom_block}' could not be converted to a block state ID");
            return Err(BlockPlaceError::BlockNotMappedToBlockStateId(bottom_block));
        };
        {
            state
                .world
                .get_or_generate_mut(context.block_position.chunk(), Dimension::Overworld)
                .expect("Could not load chunk")
                .set_block(context.block_position.chunk_block_pos(), bottom_block_id);
        }

        let upper_block = BlockData {
            name: name.to_string(),
            properties: Some(BTreeMap::from([
                ("facing".to_string(), facing.to_string()),
                ("half".to_string(), "upper".to_string()),
                ("hinge".to_string(), "left".to_string()),
                ("open".to_string(), "false".to_string()),
                ("powered".to_string(), "false".to_string()),
            ])),
        };

        let Some(upper_block_id) = upper_block.clone().try_to_block_state_id() else {
            error!("Block data '{upper_block}' could not be converted to a block state ID");
            return Err(BlockPlaceError::BlockNotMappedToBlockStateId(upper_block));
        };
        {
            state
                .world
                .get_or_generate_mut(context.block_position.chunk(), Dimension::Overworld)
                .expect("Could not load chunk")
                .set_block(
                    (context.block_position + (0, 1, 0)).chunk_block_pos(),
                    upper_block_id,
                );
        }

        Ok(PlacedBlocks {
            blocks: std::collections::HashMap::from([
                (context.block_position, bottom_block_id),
                (context.block_position + (0, 1, 0), upper_block_id),
            ]),
            take_item: true,
        })
    }
}
