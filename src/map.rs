use crate::units::{
    Player, Unit,
    movements::{Direction, JustTeleported, TileMovement},
};
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
            .insert_resource(MultiMapManager::default())
            .add_systems(
                FixedUpdate,
                (
                    clear_teleport_status_system,
                    portal_activation_system,
                    update_entity_maps_system,
                    spawn_chunks_around_camera_system,
                    spawn_chunks_around_units_system,
                )
                    .chain(),
            )
            .add_systems(
                Update,
                (camera_follow_active_map_system, render_active_map_system),
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

#[derive(Default, Debug)]
pub struct ChunkManager {
    pub spawned_chunks: HashMap<ChunkPos, Entity>, // ChunkPos -> chunk
}

/// to quickly find the Structure at coordinates without checking every Structure
#[derive(Default, Debug)]
pub struct StructureManager {
    pub structures: HashMap<GridPos, Entity>, // GridPos -> structure
}

/// Identifiant unique pour chaque map
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MapId(pub u32);

impl Default for MapId {
    fn default() -> Self {
        MapId(0) // Map principale
    }
}

/// Types de maps possibles
#[derive(Debug, Clone)]
pub enum MapType {
    Overworld,
    House { owner_id: Option<Entity> },
    Dungeon { level: u32 },
    Shop { shop_type: String },
}

/// Configuration d'une map
#[derive(Debug, Clone)]
pub struct MapConfig {
    pub id: MapId,
    pub name: String,
    pub map_type: MapType,
    pub size: UVec2, // Taille de la map en chunks
    pub spawn_point: GridPos,
}

/// Composant pour marquer les entités qui peuvent servir de portail
#[derive(Component)]
pub struct Portal {
    pub target_map: MapId,
    pub target_position: GridPos,
    pub facing_direction: Direction,
}

/// Composant pour marquer sur quelle map se trouve une entité
#[derive(Component, Debug, Clone, Copy)]
pub struct CurrentMap {
    pub map_id: MapId,
}

impl Default for CurrentMap {
    fn default() -> Self {
        CurrentMap {
            map_id: MapId::default(),
        }
    }
}

/// Resource globale pour gérer toutes les maps
#[derive(Resource)]
pub struct MultiMapManager {
    pub maps: HashMap<MapId, MapData>,
    pub active_map: MapId, // Map actuellement affichée
    pub next_map_id: u32,
}

/// Données spécifiques à chaque map
pub struct MapData {
    pub config: MapConfig,
    pub chunk_manager: ChunkManager,
    pub structure_manager: StructureManager,
    pub entities: Vec<Entity>, // Entités présentes sur cette map
    pub portals: Vec<Entity>,  // Portails de cette map
}

impl Default for MultiMapManager {
    fn default() -> Self {
        let mut maps = HashMap::new();

        // Map principale (overworld)
        let main_map = MapData {
            config: MapConfig {
                id: MapId(0),
                name: "Overworld".to_string(),
                map_type: MapType::Overworld,
                size: UVec2::new(100, 100),
                spawn_point: GridPos { x: 0, y: 0 },
            },
            chunk_manager: ChunkManager::default(),
            structure_manager: StructureManager::default(),
            entities: Vec::new(),
            portals: Vec::new(),
        };
        maps.insert(MapId(0), main_map);

        Self {
            maps,
            active_map: MapId(0),
            next_map_id: 1,
        }
    }
}

impl MultiMapManager {
    /// Crée une nouvelle map
    pub fn create_map(&mut self, config: MapConfig) -> MapId {
        let map_id = MapId(self.next_map_id);
        self.next_map_id += 1;

        let map_data = MapData {
            config: MapConfig {
                id: map_id,
                ..config
            },
            chunk_manager: ChunkManager::default(),
            structure_manager: StructureManager::default(),
            entities: Vec::new(),
            portals: Vec::new(),
        };

        self.maps.insert(map_id, map_data);
        map_id
    }

    /// Ajoute un portail entre deux maps
    pub fn add_portal(&mut self, portal_entity: Entity, from_map: MapId, to_map: MapId) {
        if let Some(map_data) = self.maps.get_mut(&from_map) {
            map_data.portals.push(portal_entity);
        }
    }

    /// Déplace une entité d'une map à une autre
    pub fn transfer_entity(&mut self, entity: Entity, from_map: MapId, to_map: MapId) {
        // Retirer de l'ancienne map
        if let Some(from_data) = self.maps.get_mut(&from_map) {
            from_data.entities.retain(|&e| e != entity);
        }

        // Ajouter à la nouvelle map
        if let Some(to_data) = self.maps.get_mut(&to_map) {
            to_data.entities.push(entity);
        }
    }

    /// Change la map active (celle qui est affichée)
    pub fn set_active_map(&mut self, map_id: MapId) {
        if self.maps.contains_key(&map_id) {
            self.active_map = map_id;
        }
    }

    /// Récupère les données d'une map
    pub fn get_map(&self, map_id: MapId) -> Option<&MapData> {
        self.maps.get(&map_id)
    }

    /// Récupère les données d'une map (mutable)
    pub fn get_map_mut(&mut self, map_id: MapId) -> Option<&mut MapData> {
        self.maps.get_mut(&map_id)
    }
}

// fn clear_teleport_status_system(
//     mut commands: Commands,
//     mut moved_units: Query<(Entity, &GridPos, &mut JustTeleported), Changed<GridPos>>,
// ) {
//     for (entity, _, _) in moved_units.iter_mut() {
//         commands.entity(entity).remove::<JustTeleported>();
//     }
// }
fn clear_teleport_status_system(
    mut commands: Commands,
    mut moved_units: Query<(Entity, &GridPos, &mut JustTeleported)>,
    mut previous_positions: Local<HashMap<Entity, GridPos>>,
) {
    for (entity, current_pos, mut just_teleported) in moved_units.iter_mut() {
        if let Some(previous_pos) = previous_positions.get(&entity) {
            // Vérifie si l'unité s'est vraiment déplacée (pas juste téléportée)
            if previous_pos != current_pos {
                just_teleported.steps_taken += 1;

                // Retire le composant après au moins 1 mouvement volontaire
                if just_teleported.steps_taken >= 1 {
                    commands.entity(entity).remove::<JustTeleported>();
                }
            }
        }
        // Met à jour la position précédente
        previous_positions.insert(entity, *current_pos);
    }
}

fn portal_activation_system(
    mut commands: Commands,
    mut multi_map_manager: ResMut<MultiMapManager>,
    portal_query: Query<(Entity, &Portal, &GridPos, &CurrentMap), Without<Unit>>,
    mut unit_query: Query<(Entity, &mut GridPos, &mut CurrentMap, Has<Player>), With<Unit>>,
) {
    for (unit_entity, mut unit_pos, mut unit_map, is_player) in unit_query.iter_mut() {
        for (portal_entity, portal, portal_pos, portal_map) in portal_query.iter() {
            if portal_map.map_id != unit_map.map_id {
                continue;
            }

            let distance =
                ((unit_pos.x - portal_pos.x).abs() + (unit_pos.y - portal_pos.y).abs()) as u8;
            // if distance <= portal.activation_range {
            if distance <= 0 {
                // Calculez la direction d'arrivée relative au portail
                let approach_direction = calculate_approach_direction(*unit_pos, *portal_pos);

                // Téléportez vers une position adjacent au portail de destination
                let exit_position = calculate_exit_position(
                    portal.target_position,
                    portal.facing_direction,
                    approach_direction,
                );

                // Transférer l'entité
                multi_map_manager.transfer_entity(unit_entity, unit_map.map_id, portal.target_map);
                *unit_pos = exit_position;
                unit_map.map_id = portal.target_map;

                if is_player {
                    multi_map_manager.active_map = unit_map.map_id;
                }
                break;
            }
        }
    }
}

fn calculate_approach_direction(unit_pos: GridPos, portal_pos: GridPos) -> Direction {
    let delta = IVec2::new(unit_pos.x - portal_pos.x, unit_pos.y - portal_pos.y);

    // Détermine la direction principale d'approche
    if delta.x.abs() > delta.y.abs() {
        if delta.x > 0 {
            Direction::West
        } else {
            Direction::East
        }
    } else {
        if delta.y > 0 {
            Direction::South
        } else {
            Direction::North
        }
    }
}

fn calculate_exit_position(
    target_pos: GridPos,
    portal_facing: Direction,
    approach_direction: Direction,
) -> GridPos {
    // Par défaut, sortir devant le portail
    let mut exit_pos = target_pos + portal_facing.delta();

    // Ajuster selon la direction d'arrivée pour éviter les collisions
    match approach_direction {
        Direction::North => exit_pos.y += 1,
        Direction::South => exit_pos.y -= 1,
        Direction::East => exit_pos.x -= 1,
        Direction::West => exit_pos.x += 1,
        _ => {}
    }

    exit_pos
}

/// Système pour maintenir à jour les listes d'entités par map
fn update_entity_maps_system(
    mut multi_map_manager: ResMut<MultiMapManager>,
    entity_query: Query<(Entity, &CurrentMap), Changed<CurrentMap>>,
) {
    for (entity, current_map) in entity_query.iter() {
        // S'assurer que l'entité est dans la bonne liste
        if let Some(map_data) = multi_map_manager.get_map_mut(current_map.map_id) {
            if !map_data.entities.contains(&entity) {
                map_data.entities.push(entity);
            }
        }
    }
}

/// Système pour faire suivre la caméra à la map active
fn camera_follow_active_map_system(
    multi_map_manager: Res<MultiMapManager>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
    player_query: Query<(&GridPos, &CurrentMap), (With<Unit>, With<crate::units::Player>)>,
) {
    if let Ok((player_pos, player_map)) = player_query.get_single() {
        if player_map.map_id == multi_map_manager.active_map {
            // La caméra suit le joueur normalement
        } else {
            // Le joueur a changé de map, mettre à jour la map active
            // (cela pourrait être fait dans portal_activation_system)
        }
    }
}

/// Système pour n'afficher que la map active
fn render_active_map_system(
    multi_map_manager: Res<MultiMapManager>,
    mut entity_query: Query<(&CurrentMap, &mut Visibility)>,
) {
    for (current_map, mut visibility) in entity_query.iter_mut() {
        if current_map.map_id == multi_map_manager.active_map {
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

// Fonctions utilitaires pour créer des maps et portails

/// Crée une maison avec un portail d'entrée
pub fn create_house_map(
    commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    multi_map_manager: &mut ResMut<MultiMapManager>,
    house_entrance_pos: GridPos,
    main_map_id: MapId,
) -> MapId {
    // Créer la config de la maison
    let house_config = MapConfig {
        id: MapId(0), // Sera remplacé par create_map
        name: "House Interior".to_string(),
        map_type: MapType::House { owner_id: None },
        size: UVec2::new(10, 10),
        spawn_point: GridPos { x: 5, y: 1 }, // Près de la porte
    };

    let house_map_id = multi_map_manager.create_map(house_config);

    // Créer le portail d'entrée (sur la map principale)
    let entrance_portal = commands
        .spawn((
            Portal {
                target_map: house_map_id,
                target_position: GridPos { x: 5, y: 1 },
                facing_direction: Direction::North,
            },
            GridPos {
                x: house_entrance_pos.x,
                y: house_entrance_pos.y,
            },
            CurrentMap {
                map_id: main_map_id,
            },
            // Sprite pour visualiser la porte
            Sprite::from_image(asset_server.load("structures/door.png")),
        ))
        .id();

    // Créer le portail de sortie (dans la maison)
    let exit_portal = commands
        .spawn((
            Portal {
                target_map: main_map_id,
                target_position: GridPos {
                    x: house_entrance_pos.x,
                    y: house_entrance_pos.y - 1,
                }, // Sortir devant la maison
                facing_direction: Direction::South,
            },
            GridPos { x: 5, y: 0 }, // Position de la porte dans la maison
            CurrentMap {
                map_id: house_map_id,
            },
            Sprite::from_image(asset_server.load("structures/door.png")),
        ))
        .id();

    // Enregistrer les portails
    multi_map_manager.add_portal(entrance_portal, main_map_id, house_map_id);
    multi_map_manager.add_portal(exit_portal, house_map_id, main_map_id);

    house_map_id
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
    mut structure_manager: &mut StructureManager,
    chunk_pos: ChunkPos,
    map_id: MapId,
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
                CurrentMap { map_id },
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

    // Ajoutez aussi CurrentMap au tilemap lui-même
    commands
        .entity(tilemap_entity)
        .insert(CurrentMap { map_id });

    tilemap_entity
}

fn spawn_structure_in_chunk(
    commands: &mut Commands,
    structure_entity: &Entity,
    structure_manager: &mut StructureManager,
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
    structure_manager: &mut StructureManager,
    chunk_manager: &mut ChunkManager, // Maintenant mutable
    rounded_tile_pos: GridPos,
    map_id: MapId,
) {
    let rounded_chunk_pos = rounded_tile_pos_to_rounded_chunk(rounded_tile_pos);

    // Charger le chunk s'il n'existe pas
    if !chunk_manager
        .spawned_chunks
        .contains_key(&rounded_chunk_pos)
    {
        let entity = spawn_chunk(
            commands,
            asset_server,
            structure_manager,
            rounded_chunk_pos,
            map_id,
        );
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
    map_id: MapId,
    multi_map_manager: &Res<MultiMapManager>,
) -> bool {
    if let Some(map_data) = multi_map_manager.get_map(map_id) {
        if let Some(_structure_entity) =
            map_data.structure_manager.structures.get(&rounded_tile_pos)
        {
            return false;
        }
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
    camera_query: Query<(&Transform, &CurrentMap), With<Camera>>,
    mut multi_map_manager: ResMut<MultiMapManager>,
) {
    const SIZE: i32 = 4;
    if let Ok((transform, camera_map)) = camera_query.get_single() {
        let camera_chunk_pos = world_pos_to_rounded_chunk(transform.translation.xy());
        let active_map_id = camera_map.map_id;

        // Récupérer les données de la map de la caméra
        if let Some(map_data) = multi_map_manager.maps.get_mut(&active_map_id) {
            for y in (camera_chunk_pos.y - SIZE)..(camera_chunk_pos.y + SIZE) {
                for x in (camera_chunk_pos.x - SIZE)..(camera_chunk_pos.x + SIZE) {
                    let chunk_pos = ChunkPos { x, y };
                    if !map_data
                        .chunk_manager
                        .spawned_chunks
                        .contains_key(&chunk_pos)
                    {
                        let entity = spawn_chunk(
                            &mut commands,
                            &asset_server,
                            &mut map_data.structure_manager,
                            chunk_pos,
                            active_map_id,
                        );
                        map_data
                            .chunk_manager
                            .spawned_chunks
                            .insert(chunk_pos, entity);
                    }
                }
            }
        }
    }
}

fn spawn_chunks_around_units_system(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    unit_query: Query<(&GridPos, &CurrentMap), With<Unit>>,
    mut multi_map_manager: ResMut<MultiMapManager>,
    camera_query: Query<&CurrentMap, With<Camera>>,
) {
    const SIZE: i32 = 2;

    // Récupérer la map active (celle de la caméra)
    let active_map_id = if let Ok(camera_map) = camera_query.get_single() {
        camera_map.map_id
    } else {
        MapId(0) // Fallback vers la map principale
    };

    // Ne spawner des chunks que pour les unités sur la map active
    for (unit_grid_pos, current_map) in unit_query.iter() {
        if current_map.map_id != active_map_id {
            continue; // Ignore les unités sur d'autres maps
        }

        let unit_chunk_pos = rounded_tile_pos_to_rounded_chunk(*unit_grid_pos);

        if let Some(map_data) = multi_map_manager.maps.get_mut(&current_map.map_id) {
            for y in (unit_chunk_pos.y - SIZE)..(unit_chunk_pos.y + SIZE) {
                for x in (unit_chunk_pos.x - SIZE)..(unit_chunk_pos.x + SIZE) {
                    let chunk_pos = ChunkPos { x, y };
                    if !map_data
                        .chunk_manager
                        .spawned_chunks
                        .contains_key(&chunk_pos)
                    {
                        let entity = spawn_chunk(
                            &mut commands,
                            &asset_server,
                            &mut map_data.structure_manager,
                            chunk_pos,
                            current_map.map_id,
                        );
                        map_data
                            .chunk_manager
                            .spawned_chunks
                            .insert(chunk_pos, entity);
                    }
                }
            }
        }
    }
}
