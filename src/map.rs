use crate::units::Unit;
use bevy::prelude::*;
use bevy_ecs_tilemap::prelude::*;
use rand::Rng;
use std::collections::HashMap;

pub const TILE_SIZE: TilemapTileSize = TilemapTileSize { x: 16.0, y: 16.0 };
pub const CHUNK_SIZE: UVec2 = UVec2 { x: 32, y: 32 };
// Render chunk sizes are set to 4 render chunks per user specified chunk.
pub const RENDER_CHUNK_SIZE: UVec2 = UVec2 {
    x: CHUNK_SIZE.x * 2,
    y: CHUNK_SIZE.y * 2,
};
pub const TILE_LAYER_LEVEL: f32 = -1.0;
pub const STRUCTURE_LAYER_LEVEL: f32 = 0.0;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_plugins(TilemapPlugin)
            .insert_resource(ChunkManager::default())
            .insert_resource(StructureManager::default())
            .add_systems(
                FixedUpdate,
                (
                    spawn_chunks_around_camera_system,
                    spawn_chunks_around_units_system,
                ),
            );
    }
}

#[derive(Component, Default, Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

impl GridPos {
    pub fn to_chunk_pos(self) -> ChunkPos {
        ChunkPos {
            x: self.x * CHUNK_SIZE.x as i32,
            y: self.y * CHUNK_SIZE.y as i32,
        }
    }

    pub fn into_IVec2(self) -> IVec2 {
        IVec2 {
            x: self.x,
            y: self.y,
        }
    }
}

impl From<ChunkPos> for GridPos {
    fn from(pos: ChunkPos) -> Self {
        Self {
            x: pos.x * CHUNK_SIZE.x as i32,
            y: pos.y * CHUNK_SIZE.y as i32,
        }
    }
}

impl std::ops::Add<IVec2> for GridPos {
    type Output = Self;

    fn add(self, left: IVec2) -> Self {
        Self {
            x: self.x + left.x,
            y: self.y + left.y,
        }
    }
}

impl std::ops::Sub<IVec2> for GridPos {
    type Output = Self;

    fn sub(self, left: IVec2) -> Self {
        Self {
            x: self.x - left.x,
            y: self.y - left.y,
        }
    }
}

/// ChunkPos {x: 2, y: 2} <=> GridPos {x: 2*CHUNK_SIZE, y: 2*CHUNK_SIZE}
#[derive(Component, Default, Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub struct ChunkPos {
    pub x: i32,
    pub y: i32,
}

impl From<GridPos> for ChunkPos {
    fn from(pos: GridPos) -> Self {
        Self {
            x: pos.x / CHUNK_SIZE.x as i32,
            y: pos.y / CHUNK_SIZE.y as i32,
        }
    }
}

#[derive(Resource, Default, Debug)]
pub struct ChunkManager {
    pub spawned_chunks: HashMap<ChunkPos, Entity>, // ChunkPos -> chunk
}

/// to quickly find the Structure at coordinates without checking every Structure
#[derive(Resource, Default, Debug)]
pub struct StructureManager {
    pub structures: HashMap<GridPos, Entity>, // GridPos -> structure
}

#[derive(Component)]
pub struct Structure;

#[derive(Component)]
pub struct Wall;

#[derive(Component)]
pub struct Chest;
// TODO: delete these two component and do something better
#[derive(Component)]
pub struct Provider;
#[derive(Component)]
pub struct Requester;

#[derive(Component)]
pub struct Crafter;

pub fn spawn_chunk(
    commands: &mut Commands,
    asset_server: &AssetServer,
    mut structure_manager: &mut ResMut<StructureManager>,
    chunk_pos: ChunkPos,
) -> Entity {
    let tilemap_entity = commands.spawn_empty().id();
    let mut tile_storage = TileStorage::empty(CHUNK_SIZE.into());
    let mut rng = rand::rng();

    // Collecte les positions des structures à créer
    let mut structures_to_spawn = Vec::new();

    // Spawn the elements of the tilemap.
    for x in 0..CHUNK_SIZE.x {
        for y in 0..CHUNK_SIZE.y {
            let local_tile_pos = TilePos { x, y };
            let tile_entity = commands
                .spawn(TileBundle {
                    position: local_tile_pos,
                    tilemap_id: TilemapId(tilemap_entity),
                    texture_index: TileTextureIndex(0),
                    ..Default::default()
                })
                .id();

            let is_wall = rng.random_bool(0.2);
            if is_wall
                && (chunk_pos.x > 0 || chunk_pos.x < 0)
                && (chunk_pos.y > 0 || chunk_pos.y < 0)
            {
                let local_tile_pos = GridPos {
                    x: local_tile_pos.x as i32,
                    y: local_tile_pos.y as i32,
                };
                let rounded_tile_pos = local_tile_pos_to_rounded_tile(local_tile_pos, chunk_pos);
                structures_to_spawn.push(rounded_tile_pos);
            }

            match commands.get_entity(tilemap_entity) {
                Ok(mut entity_command) => entity_command.add_child(tile_entity),
                Err(_) => todo!(),
            };

            tile_storage.set(&local_tile_pos, tile_entity);
        }
    }

    // Calcule la position du tilemap dans le monde
    // let rounded_tile_pos = rounded_chunk_pos_to_rounded_tile(&chunk_pos);
    let rounded_tile_pos = GridPos::from(chunk_pos);
    let tilemap_world_pos = rounded_tile_pos_to_world(rounded_tile_pos);
    let tilemap_transform = Transform::from_translation(Vec3::new(
        tilemap_world_pos.x,
        tilemap_world_pos.y,
        TILE_LAYER_LEVEL,
    ));

    let image_handles = vec![
        asset_server.load("tiles/grass.png"),
        asset_server.load("tiles/stone.png"),
    ];

    // Configure le tilemap
    match commands.get_entity(tilemap_entity) {
        Ok(mut entity_commands) => entity_commands.insert(TilemapBundle {
            grid_size: TILE_SIZE.into(),
            size: CHUNK_SIZE.into(),
            storage: tile_storage,
            texture: TilemapTexture::Vector(image_handles),
            tile_size: TILE_SIZE,
            transform: tilemap_transform,
            render_settings: TilemapRenderSettings {
                render_chunk_size: RENDER_CHUNK_SIZE,
                ..Default::default()
            },
            ..Default::default()
        }),
        Err(_) => todo!(),
    };

    // Spawn les structures APRÈS avoir configuré le tilemap
    // et les attache directement au tilemap
    for rounded_tile_pos in structures_to_spawn {
        let wall_entity = commands
            .spawn((
                Structure,
                Wall,
                Sprite::from_image(asset_server.load("structures/wall.png")),
            ))
            .id();

        spawn_structure_in_chunk(
            commands,
            &wall_entity,
            &mut structure_manager,
            tilemap_entity,
            rounded_tile_pos,
            tilemap_world_pos,
        );
    }

    tilemap_entity
}

fn spawn_structure_in_chunk(
    commands: &mut Commands,
    structure_entity: &Entity,
    structure_manager: &mut ResMut<StructureManager>,
    tilemap_entity: Entity,
    rounded_tile_pos: GridPos,
    tilemap_world_pos: Vec2,
) {
    // Calcule la position absolue de la structure
    let structure_world_pos = rounded_tile_pos_to_world(rounded_tile_pos);

    // Calcule la position RELATIVE au tilemap
    let relative_pos = structure_world_pos - tilemap_world_pos;

    let transform = Transform::from_translation(Vec3::new(
        relative_pos.x,
        relative_pos.y,
        STRUCTURE_LAYER_LEVEL - TILE_LAYER_LEVEL, // Z relatif
    ));

    // TODO: use GridPos instead of Transform there
    let global_grid_pos = GridPos {
        x: rounded_tile_pos.x,
        y: rounded_tile_pos.y,
    };

    match commands.get_entity(*structure_entity) {
        Ok(mut entity_command) => {
            entity_command.insert(transform);
            entity_command.insert(global_grid_pos);
        }
        Err(_) => todo!(),
    };

    // Attache la structure au tilemap, pas à une tile individuelle
    match commands.get_entity(tilemap_entity) {
        Ok(mut entity_command) => entity_command.add_child(*structure_entity),
        Err(_) => todo!(),
    };

    // Enregistre la structure dans le manager
    structure_manager
        .structures
        .insert(rounded_tile_pos, *structure_entity);
}

// add transform to structure_entity and add it to structure_manager
pub fn place_structure(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>, // Ajouté pour pouvoir spawner le chunk
    structure_entity: &Entity,
    structure_manager: &mut ResMut<StructureManager>,
    chunk_manager: &mut ResMut<ChunkManager>, // Maintenant mutable
    rounded_tile_pos: GridPos,
) {
    let rounded_chunk_pos = rounded_tile_pos_to_rounded_chunk(rounded_tile_pos);

    // Charger le chunk s'il n'existe pas
    if !chunk_manager
        .spawned_chunks
        .contains_key(&rounded_chunk_pos)
    {
        let entity = spawn_chunk(commands, asset_server, structure_manager, rounded_chunk_pos);
        chunk_manager
            .spawned_chunks
            .insert(rounded_chunk_pos, entity);
    }

    // Maintenant le chunk existe forcément
    if let Some(&tilemap_entity) = chunk_manager.spawned_chunks.get(&rounded_chunk_pos) {
        let tilemap_world_pos =
            rounded_tile_pos_to_world(rounded_chunk_pos_to_rounded_tile(rounded_chunk_pos));

        spawn_structure_in_chunk(
            commands,
            structure_entity,
            structure_manager,
            tilemap_entity,
            rounded_tile_pos,
            tilemap_world_pos,
        );
    } else {
        panic!();
    }
}

pub fn get_neighbors(pos: GridPos) -> impl Iterator<Item = GridPos> {
    (-1..=1)
        .flat_map(move |x| (-1..=1).map(move |y| (x, y)))
        .filter(|&(x, y)| x != 0 || y != 0)
        .map(move |(dx, dy)| GridPos {
            x: pos.x + dx,
            y: pos.y + dy,
        })
}

pub fn is_tile_passable(
    rounded_tile_pos: GridPos,
    structure_manager: &Res<StructureManager>,
) -> bool {
    if let Some(_structure_entity) = structure_manager.structures.get(&rounded_tile_pos) {
        return false;
    }
    // Si le chunk n'existe pas, on suppose qu'il n'y a pas de mur.
    // TODO: change that or spawn the chunk
    true
}

// ========= coordinates conversion =========
// world_pos = (5.5 * TILE_SIZE.X, 0.5 * TILE_SIZE.y) | tile_pos = (5.5, 0.5) | rounded_tile_pos = (5, 0)

pub fn local_tile_pos_to_rounded_tile(
    local_tile_pos: GridPos,
    rounded_chunk_pos: ChunkPos,
) -> GridPos {
    GridPos {
        x: rounded_chunk_pos.x * CHUNK_SIZE.x as i32 + local_tile_pos.x,
        y: rounded_chunk_pos.y * CHUNK_SIZE.y as i32 + local_tile_pos.y,
    }
}

// Conversion coordonnées logiques -> monde ; (5.5, 0.5) => (5.5 * TILE_SIZE.x, 0.5 * TILE_SIZE.y)
pub fn tile_pos_to_world(tile_pos: Vec2) -> Vec2 {
    Vec2::new(tile_pos.x * TILE_SIZE.x, tile_pos.y * TILE_SIZE.y)
}

// adds 0.5 to coordinates to make entities spawn based on the corner of there sprite and not the center
pub fn rounded_tile_pos_to_world(rounded_tile_pos: GridPos) -> Vec2 {
    Vec2::new(
        rounded_tile_pos.x as f32 * TILE_SIZE.x + 0.5 * TILE_SIZE.x,
        rounded_tile_pos.y as f32 * TILE_SIZE.y + 0.5 * TILE_SIZE.y,
    )
}

// (5.5, 0.5) => (5, 0)
pub fn tile_pos_to_rounded_tile(tile_pos: Vec2) -> GridPos {
    GridPos {
        x: tile_pos.x.floor() as i32,
        y: tile_pos.y.floor() as i32,
    }
}

// Conversion monde -> coordonnées logiques
pub fn world_pos_to_tile(world_pos: Vec2) -> Vec2 {
    Vec2::new(world_pos.x / TILE_SIZE.x, world_pos.y / TILE_SIZE.y)
}

// Conversion monde -> coordonnées logiques
pub fn world_pos_to_rounded_tile(world_pos: Vec2) -> GridPos {
    GridPos {
        x: (world_pos.x / TILE_SIZE.x).floor() as i32,
        y: (world_pos.y / TILE_SIZE.y).floor() as i32,
    }
}

/// Convertit une position monde (pixels) en position de chunk.
pub fn world_pos_to_rounded_chunk(world_pos: Vec2) -> ChunkPos {
    ChunkPos {
        x: (world_pos.x / (CHUNK_SIZE.x as f32 * TILE_SIZE.x)).floor() as i32,
        y: (world_pos.y / (CHUNK_SIZE.y as f32 * TILE_SIZE.y)).floor() as i32,
    }
}

pub fn rounded_chunk_pos_to_rounded_tile(rounded_chunk_pos: ChunkPos) -> GridPos {
    GridPos {
        x: rounded_chunk_pos.x * CHUNK_SIZE.x as i32,
        y: rounded_chunk_pos.y * CHUNK_SIZE.y as i32,
    }
}

pub fn rounded_tile_pos_to_rounded_chunk(rounded_tile_pos: GridPos) -> ChunkPos {
    ChunkPos {
        x: rounded_tile_pos.x / CHUNK_SIZE.x as i32,
        y: rounded_tile_pos.y / CHUNK_SIZE.y as i32,
    }
}

pub fn tile_pos_to_rounded_chunk(tile_pos: Vec2) -> ChunkPos {
    ChunkPos {
        x: (tile_pos.x / CHUNK_SIZE.x as f32).floor() as i32,
        y: (tile_pos.y / CHUNK_SIZE.y as f32).floor() as i32,
    }
}
// ==========================================

fn spawn_chunks_around_camera_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    camera_query: Query<&Transform, With<Camera>>,
    mut chunk_manager: ResMut<ChunkManager>,
    mut structure_manager: ResMut<StructureManager>,
) {
    const SIZE: i32 = 4;
    if let Ok(transform) = camera_query.single() {
        let camera_chunk_pos = world_pos_to_rounded_chunk(transform.translation.xy());
        for y in (camera_chunk_pos.y - SIZE)..(camera_chunk_pos.y + SIZE) {
            for x in (camera_chunk_pos.x - SIZE)..(camera_chunk_pos.x + SIZE) {
                let chunk_pos = ChunkPos { x, y };
                if !chunk_manager.spawned_chunks.contains_key(&chunk_pos) {
                    let entity = spawn_chunk(
                        &mut commands,
                        &asset_server,
                        &mut structure_manager,
                        chunk_pos,
                    );
                    chunk_manager.spawned_chunks.insert(chunk_pos, entity);
                }
            }
        }
    }
}

fn spawn_chunks_around_units_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    unit_query: Query<&GridPos, With<Unit>>,
    mut chunk_manager: ResMut<ChunkManager>,
    mut structure_manager: ResMut<StructureManager>,
) {
    const SIZE: i32 = 2;
    for unit_grid_pos in unit_query.iter() {
        let unit_chunk_pos = rounded_tile_pos_to_rounded_chunk(*unit_grid_pos);
        for y in (unit_chunk_pos.y - SIZE)..(unit_chunk_pos.y + SIZE) {
            for x in (unit_chunk_pos.x - SIZE)..(unit_chunk_pos.x + SIZE) {
                let chunk_pos = ChunkPos { x, y };
                if !chunk_manager.spawned_chunks.contains_key(&chunk_pos) {
                    let entity = spawn_chunk(
                        &mut commands,
                        &asset_server,
                        &mut structure_manager,
                        chunk_pos,
                    );
                    chunk_manager.spawned_chunks.insert(chunk_pos, entity);
                }
            }
        }
    }
}
