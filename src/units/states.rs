use bevy::prelude::*;

/// example: when units.tasks_queue.is_empty() && no currentTask
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Available;
