use crate::{
    items::Inventory,
    map::GridPos,
    pathfinding::PathfindingAgent,
    units::{
        movements::{
            Direction, TileMovement, move_and_collide_units_system,
            sync_transform_to_gridpos_system, update_sprite_facing_system,
        },
        tasks::{ActionQueue, CurrentAction, CurrentTask},
    },
};
use bevy::{prelude::*, time::common_conditions::on_timer};
use rand::Rng;
use std::time::Duration;

pub const UNIT_REACH: u8 = 1;

pub struct UnitsPlugin;

impl Plugin for UnitsPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.add_systems(
            Update,
            sync_transform_to_gridpos_system.after(move_and_collide_units_system),
        )
        .add_systems(
            FixedUpdate,
            (
                // test_units_control_system.before(move_and_collide_units_system),
                move_and_collide_units_system,
                player_control_system,
                update_sprite_facing_system.after(move_and_collide_units_system),
                // display_units_with_no_current_action_system
                //     .run_if(on_timer(Duration::from_secs(5))),
                // display_units_inventory_system.run_if(on_timer(Duration::from_secs(5))),
            ),
        );
    }
}

#[derive(Component, Debug, Default)]
#[require(
    Sprite,
    // Transform,
    GridPos,
    TileMovement,
    PathfindingAgent,
    Inventory,
    ActionQueue,
    CurrentAction,
    CurrentTask
)]
pub struct Unit {
    pub name: String,
}

#[derive(Component)]
pub struct Player;

/// add if the unit should checks its collisions with other units (collisions with walls are not affected by this component)
#[derive(Component)]
pub struct UnitUnitCollisions;

pub fn player_control_system(
    mut unit_query: Query<&mut TileMovement, (With<Unit>, With<Player>)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    if let Ok(mut tile_movement) = unit_query.single_mut() {
        let mut delta = IVec2::new(0, 0);
        if input.pressed(KeyCode::KeyW) {
            delta.y += 1;
        }
        if input.pressed(KeyCode::KeyA) {
            delta.x -= 1;
        }
        if input.pressed(KeyCode::KeyD) {
            delta.x += 1;
        }
        if input.pressed(KeyCode::KeyS) {
            delta.y -= 1;
        }
        let new_direction = Direction::from(delta);

        if tile_movement.direction != new_direction {
            tile_movement.direction = new_direction;
        }
    }
}

pub fn display_units_with_no_current_action_system(unit_query: Query<&CurrentAction, With<Unit>>) {
    let mut counter = 0;
    for current_action in unit_query.iter() {
        if current_action.action.is_none() {
            counter += 1;
        }
    }
    println!("Counter units with no current action: {}", counter);
}

pub fn display_units_inventory_system(unit_query: Query<&Inventory>) {
    for inventory in unit_query.iter() {
        if !inventory.stackable_items.is_empty() {
            println!("{:?}", inventory);
        }
    }
}

pub fn test_units_control_system(
    mut unit_query: Query<&mut TileMovement, (With<Unit>, Without<Player>)>,
) {
    let mut rng = rand::rng();
    for mut tile_movement in unit_query.iter_mut() {
        let random = rng.random_range(1..=8);

        let new_direction = match random {
            1 => Direction::NorthWest,
            2 => Direction::North,
            3 => Direction::NorthEast,
            4 => Direction::East,
            5 => Direction::SouthEast,
            6 => Direction::South,
            7 => Direction::SouthWest,
            8 => Direction::West,
            _ => Direction::Null,
        };

        if tile_movement.direction != new_direction {
            tile_movement.direction = new_direction;
        }
    }
}
