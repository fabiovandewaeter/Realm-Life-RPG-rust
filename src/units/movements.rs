use crate::{
    UPS_TARGET,
    map::{
        GridPos, Structure, StructureManager, is_tile_passable, rounded_tile_pos_to_world,
        world_pos_to_rounded_tile,
    },
    units::{Unit, UnitUnitCollisions},
};
use bevy::prelude::*;
use std::collections::HashSet;

pub const UNIT_DEFAULT_MOVEMENT_SPEED: u32 = UPS_TARGET as u32; // ticks per tile ; smaller is faster (here its 1 tile per second at normal tickrate by default)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Null,
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

impl Direction {
    /// Retourne le déplacement (dx, dy) associé à la direction
    pub fn delta(&self) -> IVec2 {
        match self {
            Direction::Null => IVec2::ZERO,
            Direction::NorthWest => IVec2::new(-1, 1),
            Direction::North => IVec2::new(0, 1),
            Direction::NorthEast => IVec2::new(1, 1),
            Direction::East => IVec2::new(1, 0),
            Direction::SouthEast => IVec2::new(1, -1),
            Direction::South => IVec2::new(0, -1),
            Direction::SouthWest => IVec2::new(-1, -1),
            Direction::West => IVec2::new(-1, 0),
        }
    }

    pub fn from(delta: IVec2) -> Self {
        match delta {
            IVec2 { x: 0, y: 0 } => Direction::Null,
            IVec2 { x: -1, y: 1 } => Direction::NorthWest,
            IVec2 { x: 0, y: 1 } => Direction::North,
            IVec2 { x: 1, y: 1 } => Direction::NorthEast,
            IVec2 { x: 1, y: 0 } => Direction::East,
            IVec2 { x: 1, y: -1 } => Direction::SouthEast,
            IVec2 { x: 0, y: -1 } => Direction::South,
            IVec2 { x: -1, y: -1 } => Direction::SouthWest,
            IVec2 { x: -1, y: 0 } => Direction::West,
            _ => Direction::Null, // if direction is wrong
        }
    }
}

#[derive(Component)]
pub struct TileMovement {
    pub direction: Direction,
    ticks_per_tile: u32, // movement speed ; smaller is faster
    pub tick_counter: u32,
}

impl Default for TileMovement {
    fn default() -> Self {
        Self {
            direction: Direction::Null,
            ticks_per_tile: UNIT_DEFAULT_MOVEMENT_SPEED,
            tick_counter: 0,
        }
    }
}

impl TileMovement {
    pub fn new(ticks_per_tile: u32) -> Self {
        Self {
            direction: Direction::Null,
            ticks_per_tile,
            tick_counter: 0,
        }
    }

    pub fn update_speed(&mut self, ticks_per_tile: u32) {
        self.ticks_per_tile = ticks_per_tile;
        self.tick_counter = 0;
    }
}

pub fn move_and_collide_units_system(
    structure_manager: Res<StructureManager>,
    mut unit_query: Query<
        (
            Entity,
            &mut GridPos,
            &mut TileMovement,
            Has<UnitUnitCollisions>,
        ),
        With<Unit>,
    >,
) {
    // Collecte des tiles occupées en une seule passe
    let occupied_tiles = collect_occupied_tiles(&unit_query);

    // Traitement des mouvements
    for (entity, mut grid_pos, mut tile_movement, has_unit_collisions) in unit_query.iter_mut() {
        if !should_process_movement(&mut tile_movement) {
            continue;
        }

        match calculate_movement(
            *grid_pos,
            tile_movement.direction,
            &structure_manager,
            &occupied_tiles,
            has_unit_collisions,
        ) {
            MovementResult::Success {
                target_tile,
                new_direction,
            } => {
                grid_pos.x = target_tile.x;
                grid_pos.y = target_tile.y;
                tile_movement.direction = new_direction;
            }
            MovementResult::Blocked => {
                tile_movement.direction = Direction::Null;
            }
        }
    }
}

pub fn sync_transform_to_gridpos_system(
    mut query: Query<(&GridPos, &mut Transform), Without<Structure>>,
    time: Res<Time>,
) {
    for (grid_pos, mut transform) in query.iter_mut() {
        let target_pos = rounded_tile_pos_to_world(*grid_pos);
        let current_pos = transform.translation.xy();

        // Interpolation linéaire simple
        let new_pos = current_pos.lerp(target_pos, time.delta_secs() * 10.0);
        transform.translation.x = new_pos.x;
        transform.translation.y = new_pos.y;
        // transform.translation.x = target_pos.x;
        // transform.translation.y = target_pos.y;
    }
}

// Structs et enums pour clarifier les intentions
#[derive(Debug)]
enum MovementResult {
    Success {
        target_tile: GridPos,
        new_direction: Direction,
    },
    Blocked,
}

// Fonctions helper pour séparer les responsabilités
fn collect_occupied_tiles(
    unit_query: &Query<
        (
            Entity,
            &mut GridPos,
            &mut TileMovement,
            Has<UnitUnitCollisions>,
        ),
        With<Unit>,
    >,
) -> HashSet<GridPos> {
    unit_query
        .iter()
        .filter(|(_, _, _, has_collisions)| *has_collisions)
        .map(|(_, grid_pos, _, _)| *grid_pos)
        .collect()
}

fn should_process_movement(tile_movement: &mut TileMovement) -> bool {
    if tile_movement.direction == Direction::Null {
        return false;
    }

    tile_movement.tick_counter += 1;

    if tile_movement.tick_counter >= tile_movement.ticks_per_tile {
        tile_movement.tick_counter = 0;
        true
    } else {
        false
    }
}

fn calculate_movement(
    current_tile: GridPos,
    desired_direction: Direction,
    structure_manager: &Res<StructureManager>,
    occupied_tiles: &HashSet<GridPos>,
    has_unit_collisions: bool,
) -> MovementResult {
    let desired_delta = desired_direction.delta();
    let desired_target = current_tile + desired_delta;

    // Vérification spéciale pour les mouvements diagonaux
    if is_diagonal_movement(desired_delta) {
        if let Some(result) = handle_diagonal_movement(
            current_tile,
            desired_delta,
            structure_manager,
            occupied_tiles,
            has_unit_collisions,
        ) {
            return result;
        }
    }

    // Mouvement direct possible ?
    if can_move_to(
        desired_target,
        structure_manager,
        occupied_tiles,
        has_unit_collisions,
    ) {
        return MovementResult::Success {
            target_tile: desired_target,
            new_direction: desired_direction,
        };
    }

    // Pour les mouvements diagonaux, essayer les axes séparément
    if is_diagonal_movement(desired_delta) {
        try_axis_movement(
            current_tile,
            desired_delta,
            structure_manager,
            occupied_tiles,
            has_unit_collisions,
        )
    } else {
        MovementResult::Blocked
    }
}

fn is_diagonal_movement(delta: IVec2) -> bool {
    delta.x != 0 && delta.y != 0
}

fn handle_diagonal_movement(
    current_tile: GridPos,
    desired_delta: IVec2,
    structure_manager: &Res<StructureManager>,
    occupied_tiles: &HashSet<GridPos>,
    has_unit_collisions: bool,
) -> Option<MovementResult> {
    let tile_x = current_tile + IVec2::new(desired_delta.x, 0);
    let tile_y = current_tile + IVec2::new(0, desired_delta.y);

    // Interdiction si les deux cases orthogonales sont bloquées
    let can_move_x = can_move_to(
        tile_x,
        structure_manager,
        occupied_tiles,
        has_unit_collisions,
    );
    let can_move_y = can_move_to(
        tile_y,
        structure_manager,
        occupied_tiles,
        has_unit_collisions,
    );

    if !can_move_x && !can_move_y {
        Some(MovementResult::Blocked)
    } else {
        None // Continue avec la logique normale
    }
}

fn try_axis_movement(
    current_tile: GridPos,
    desired_delta: IVec2,
    structure_manager: &Res<StructureManager>,
    occupied_tiles: &HashSet<GridPos>,
    has_unit_collisions: bool,
) -> MovementResult {
    let axis_y_tile = current_tile + IVec2::new(0, desired_delta.y);
    let axis_x_tile = current_tile + IVec2::new(desired_delta.x, 0);

    // Priorité Nord/Sud avant Est/Ouest
    if can_move_to(
        axis_y_tile,
        structure_manager,
        occupied_tiles,
        has_unit_collisions,
    ) {
        MovementResult::Success {
            target_tile: axis_y_tile,
            new_direction: Direction::from(IVec2::new(0, desired_delta.y)),
        }
    } else if can_move_to(
        axis_x_tile,
        structure_manager,
        occupied_tiles,
        has_unit_collisions,
    ) {
        MovementResult::Success {
            target_tile: axis_x_tile,
            new_direction: Direction::from(IVec2::new(desired_delta.x, 0)),
        }
    } else {
        MovementResult::Blocked
    }
}

fn can_move_to(
    tile: GridPos,
    structure_manager: &Res<StructureManager>,
    occupied_tiles: &HashSet<GridPos>,
    has_unit_collisions: bool,
) -> bool {
    is_tile_passable(tile, structure_manager)
        && (!has_unit_collisions || !occupied_tiles.contains(&tile))
}

pub fn update_sprite_facing_system(mut query: Query<(&TileMovement, &mut Transform)>) {
    for (movement, mut transform) in query.iter_mut() {
        if movement.direction != Direction::Null {
            // Détermine la direction horizontale
            let is_moving_left = matches!(
                movement.direction,
                Direction::West | Direction::NorthWest | Direction::SouthWest
            );

            let is_moving_right = matches!(
                movement.direction,
                Direction::East | Direction::NorthEast | Direction::SouthEast
            );

            if is_moving_left {
                transform.scale.x = -transform.scale.x.abs();
            } else if is_moving_right {
                transform.scale.x = transform.scale.x.abs();
            }
        }
    }
}
