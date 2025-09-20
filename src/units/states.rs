use bevy::prelude::*;
use serde::Serialize;

/// example: when units.tasks_queue.is_empty() && no currentTask
#[derive(Component)]
#[component(storage = "SparseSet")]
pub struct Available;

#[derive(Component, Serialize)]
pub struct Emotions {
    pub colere: f32, // On utilise une valeur de 0.0 à 1.0
}
