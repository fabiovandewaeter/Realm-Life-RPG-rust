use crate::{
    camera::Selected,
    items::{CraftRecipeId, Inventory, ItemKind},
    map::{Chest, GridPos, Provider, Requester, world_pos_to_rounded_tile},
    pathfinding::PathfindingAgent,
    units::{UNIT_REACH, Unit, movements::move_and_collide_units_system, states::Available},
};
use bevy::{
    ecs::{entity, system::entity_command},
    input::common_conditions::input_pressed,
    prelude::*,
};
use std::{
    cmp::min,
    collections::{HashMap, VecDeque},
};

const MAX_TASKS_RETRIES: u32 = 3;

pub struct TasksPlugin;

impl Plugin for TasksPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        app.insert_resource(Reservations::default()).add_systems(
            FixedUpdate,
            (
                actions_decompose_planner_system.before(process_current_action_system),
                process_current_action_system.before(move_and_collide_units_system),
                update_task_completion_system.after(process_current_action_system),
                assign_next_action_or_set_available_system,
                // tests:
                test_find_2_rocks_system.run_if(input_pressed(KeyCode::KeyE)),
                test_deliver_2_rocks_system.run_if(input_pressed(KeyCode::KeyR)),
            ),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    MoveTo(GridPos),
    Craft {
        recipe: CraftRecipeId,
        quantity: u32,
        with: Entity, // crafting machine
    },
    Take {
        kind: ItemKind,
        quantity: u32,
        from: Entity, // chest
    },
    Drop {
        kind: ItemKind,
        quantity: u32,
        to: Entity, // chest
    },
}

#[derive(Component, Default, Debug)]
pub struct ActionQueue(pub VecDeque<Action>);

impl From<Vec<Action>> for ActionQueue {
    fn from(v: Vec<Action>) -> Self {
        Self(v.into())
    }
}

#[derive(Component, Default)]
pub struct CurrentAction {
    pub action: Option<Action>,
    pub initialized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TaskKind {
    Action(Action),
    GetItems { kind: ItemKind, quantity: u32 }, // take from provider chest (uses reservations)
    DeliverItems { kind: ItemKind, quantity: u32 }, // go to requester chest and drop items
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Planned, // already decomposed (actions queued)
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug)]
pub struct Task {
    pub kind: TaskKind,
    pub sub_tasks: Vec<Task>,
    pub status: TaskStatus,
}

impl Task {
    pub fn new(kind: TaskKind, sub_tasks: Vec<Task>) -> Self {
        Self {
            kind,
            sub_tasks,
            status: TaskStatus::Pending,
        }
    }
}

#[derive(Component, Default)]
pub struct CurrentTask {
    pub task: Option<Task>,
    pub initialized: bool,
}

impl CurrentTask {
    pub fn reset(&mut self) {
        self.task = None;
        self.initialized = false;
    }
}

#[derive(Resource, Default)]
pub struct Reservations {
    // chest -> owner -> kind -> qty
    pub reserved: HashMap<Entity, HashMap<Entity, HashMap<ItemKind, u32>>>,
}

impl Reservations {
    /// Try to reserve `qty` for `owner` on `chest`.
    /// This checks actual chest inventory (`chest_inv.count(kind)`) minus already reserved total.
    /// If enough free items exist, it records the reservation and returns true.
    pub fn try_reserve(
        &mut self,
        owner: Entity,
        chest: Entity,
        kind: ItemKind,
        qty: u32,
        chest_inv: &Inventory,
    ) -> bool {
        let total_reserved = self.total_reserved(chest, kind);
        let chest_available = chest_inv.count(&kind);

        // items not yet reserved
        let free = chest_available.saturating_sub(total_reserved);

        if free < qty {
            return false;
        }

        let owner_map = self.reserved.entry(chest).or_insert_with(HashMap::new);
        let kind_map = owner_map.entry(owner).or_insert_with(HashMap::new);
        *kind_map.entry(kind).or_insert(0) += qty;

        true
    }

    /// Release `qty` reserved by `owner` on `chest` for `kind`.
    /// If owner had less reserved than qty, it saturates to zero.
    pub fn release(&mut self, owner: Entity, chest: Entity, kind: ItemKind, qty: u32) {
        if let Some(owner_map) = self.reserved.get_mut(&chest) {
            if let Some(kind_map) = owner_map.get_mut(&owner) {
                if let Some(v) = kind_map.get_mut(&kind) {
                    *v = v.saturating_sub(qty);
                    if *v == 0 {
                        kind_map.remove(&kind);
                    }
                }
                if kind_map.is_empty() {
                    owner_map.remove(&owner);
                }
            }
            if owner_map.is_empty() {
                self.reserved.remove(&chest);
            }
        }
    }

    /// Release **all** reservations owned by `owner` on any chest.
    pub fn release_all_for_owner(&mut self, owner: Entity) {
        // collect chests to cleanup to avoid mutating while iterating
        let mut chests_to_clear: Vec<Entity> = Vec::new();
        for (chest_ent, owner_map) in self.reserved.iter_mut() {
            if owner_map.contains_key(&owner) {
                owner_map.remove(&owner);
            }
            if owner_map.is_empty() {
                chests_to_clear.push(*chest_ent);
            }
        }
        for chest in chests_to_clear {
            self.reserved.remove(&chest);
        }
    }

    /// Total reserved for a given chest and item kind (sum over all owners)
    pub fn total_reserved(&self, chest: Entity, kind: ItemKind) -> u32 {
        if let Some(owner_map) = self.reserved.get(&chest) {
            owner_map
                .values()
                .map(|kind_map| *kind_map.get(&kind).unwrap_or(&0))
                .sum()
        } else {
            0
        }
    }

    /// How much this owner specifically reserved on chest for kind
    pub fn owner_reserved(&self, owner: Entity, chest: Entity, kind: ItemKind) -> u32 {
        self.reserved
            .get(&chest)
            .and_then(|owners| owners.get(&owner))
            .and_then(|kmap| kmap.get(&kind))
            .cloned()
            .unwrap_or(0)
    }
}
// =================================================================

// TODO: change that to use tiles distance instead
// util: distance between tiles (euclidean)
fn tile_distance(a: GridPos, b: GridPos) -> f32 {
    let dx = (a.x - b.x) as f32;
    let dy = (a.y - b.y) as f32;
    (dx * dx + dy * dy).sqrt()
}

/// Planner: decompose Task -> Actions and attempt reservations.
/// It runs on units that have a CurrentTask (Pending) and an ActionQueue.
pub fn actions_decompose_planner_system(
    mut commands: Commands,
    mut reservations: ResMut<Reservations>,
    mut unit_query: Query<
        (
            Entity,
            &GridPos,
            &Inventory,
            &mut ActionQueue,
            &mut CurrentTask,
        ),
        (With<Unit>, With<PathfindingAgent>),
    >,
    provider_chest_query: Query<
        (Entity, &GridPos, &Inventory),
        (With<Chest>, With<Provider>, Without<Unit>),
    >,
    requester_chest_query: Query<
        (Entity, &GridPos),
        (
            With<Chest>,
            With<Requester>,
            With<Inventory>,
            Without<Provider>,
            Without<Unit>,
        ),
    >,
) {
    for (unit_ent, unit_grid_pos, unit_inv, mut action_queue, mut current_task) in
        unit_query.iter_mut()
    {
        let Some(task) = &mut current_task.task else {
            continue;
        };
        // Only decompose pending tasks
        if task.status != TaskStatus::Pending {
            continue;
        }

        match task.kind {
            TaskKind::GetItems { kind, quantity } => {
                // checks if enough in unit's inventory
                let have = unit_inv.count(&kind);
                if have >= quantity {
                    task.status = TaskStatus::Completed;
                    continue;
                }
                let needed = quantity - have;
                // let unit_tile_pos = world_pos_to_rounded_tile(transform.translation.xy());

                // find best chest taking into account reservations
                if let Some((chest_ent, chest_grid_pos, available)) = find_best_chest(
                    // unit_tile_pos,
                    *unit_grid_pos,
                    needed,
                    kind,
                    &provider_chest_query,
                    &reservations,
                ) {
                    // calculate how much we'll request to take (cap to available)
                    let take_qty = std::cmp::min(needed, available);

                    // chest inventory borrow for reservation check
                    if let Ok((ent, grid_pos, chest_inv)) = provider_chest_query.get(chest_ent) {
                        // try to reserve
                        if reservations.try_reserve(unit_ent, chest_ent, kind, take_qty, chest_inv)
                        {
                            // Plan actions: MoveTo -> Take
                            action_queue.0.push_back(Action::MoveTo(chest_grid_pos));
                            action_queue.0.push_back(Action::Take {
                                kind,
                                quantity: take_qty,
                                from: chest_ent,
                            });

                            // mark planned so we don't plan again until this task changes
                            task.status = TaskStatus::Planned;
                            current_task.initialized = true;
                            match commands.get_entity(unit_ent) {
                                Ok(mut entity_command) => entity_command.remove::<Available>(),
                                Err(_) => todo!(),
                            };
                        } else {
                            task.status = TaskStatus::Failed;
                            reservations.release_all_for_owner(unit_ent);
                        }
                    } else {
                        task.status = TaskStatus::Failed;
                    }
                } else {
                    task.status = TaskStatus::Failed;
                }
            }

            TaskKind::DeliverItems { kind, quantity } => {
                // position de l'unité
                // let unit_tile_pos = world_pos_to_rounded_tile(transform.translation.xy());

                // cherche le requester chest le plus proche (si il y en a plusieurs)
                let mut best: Option<(Entity, GridPos, f32)> = None; // (entity, grid_pos, dist)
                for (req_ent, req_grid_pos) in requester_chest_query.iter() {
                    // let req_pos = world_pos_to_rounded_tile(req_global_tf.translation().xy());
                    let dist = tile_distance(*unit_grid_pos, *req_grid_pos);
                    match &best {
                        None => best = Some((req_ent, *req_grid_pos, dist)),
                        Some((_, _, best_dist)) if dist < *best_dist => {
                            best = Some((req_ent, *req_grid_pos, dist))
                        }
                        _ => {}
                    }
                }

                if let Some((requester_ent, req_pos, _dist)) = best {
                    action_queue.0.push_back(Action::MoveTo(req_pos));
                    action_queue.0.push_back(Action::Drop {
                        kind,
                        quantity,
                        to: requester_ent,
                    });

                    task.status = TaskStatus::Planned;
                    current_task.initialized = true;
                    match commands.get_entity(unit_ent) {
                        Ok(mut entity_command) => entity_command.remove::<Available>(),
                        Err(_) => todo!(),
                    };
                } else {
                    task.status = TaskStatus::Failed;
                }
            }

            TaskKind::Action(_) => {
                if let TaskKind::Action(action) = task.kind {
                    action_queue.0.push_back(action);
                    task.status = TaskStatus::Planned;
                    match commands.get_entity(unit_ent) {
                        Ok(mut entity_command) => entity_command.remove::<Available>(),
                        Err(_) => todo!(),
                    };
                }
            }
        }
    }
}

pub fn reset_actions_system(
    action_queue: &mut ActionQueue,
    current_action: &mut CurrentAction,
    pathfinding_agent: &mut PathfindingAgent,
) {
    action_queue.0.clear();
    if let Some(action) = &current_action.action {
        match action {
            Action::MoveTo(_) => {
                pathfinding_agent.reset();
            }
            _ => {}
        };
    }
    current_action.action = None;
    current_action.initialized = false;
}

/// pops the front of the ActionQueue to get the next CurrentAction ; add Available component if unit has no more actions to do
pub fn assign_next_action_or_set_available_system(
    mut commands: Commands,
    mut unit_query: Query<
        (Entity, &mut ActionQueue, &mut CurrentAction),
        (With<Unit>, Without<Available>),
    >,
) {
    for (entity, mut action_queue, mut current_action) in unit_query.iter_mut() {
        if current_action.action.is_none() {
            if let Some(next_action) = action_queue.0.pop_front() {
                current_action.action = Some(next_action);
                current_action.initialized = false;
            } else {
                match commands.get_entity(entity) {
                    Ok(mut entity_command) => entity_command.insert(Available),
                    Err(_) => todo!(),
                };
            }
        }
    }
}

/// Executor: process current actions (Take/Drop/MoveTo)
pub fn process_current_action_system(
    mut reservations: ResMut<Reservations>,
    mut unit_query: Query<
        (
            Entity,
            &GridPos,
            &mut Inventory,
            &mut CurrentAction,
            &mut PathfindingAgent,
        ),
        With<Unit>,
    >,
    mut provider_chest_query: Query<
        (Entity, &GridPos, &mut Inventory),
        (With<Chest>, With<Provider>, Without<Unit>),
    >,
    mut requester_chest_query: Query<
        (&mut Inventory, &GridPos),
        (
            With<Chest>,
            With<Requester>,
            Without<Provider>,
            Without<Unit>,
        ),
    >,
) {
    for (unit_ent, unit_grid_pos, mut unit_inventory, mut current_action, mut pathfinding_agent) in
        unit_query.iter_mut()
    {
        if current_action.action.is_none() {
            continue;
        }
        if !current_action.initialized {
            match &current_action.action {
                Some(Action::MoveTo(target_pos)) => {
                    pathfinding_agent.reset();
                    pathfinding_agent.target = Some(*target_pos);
                }
                _ => {}
            }
            current_action.initialized = true;
        }

        if let Some(action) = &mut current_action.action {
            match action {
                Action::MoveTo(target_pos) => {
                    // let current_unit_tile_pos = world_pos_to_rounded_tile(unit_transform.translation.xy());
                    // let distance = current_unit_tile_pos.distance(*target_pos);
                    let distance = tile_distance(*unit_grid_pos, *target_pos);

                    // Si on est assez proche de la destination, considérer la tâche terminée
                    if pathfinding_agent.path.is_empty() && distance as u8 <= UNIT_REACH {
                        current_action.action = None;
                        pathfinding_agent.reset();
                    }
                }

                Action::Take {
                    kind,
                    quantity,
                    from,
                } => {
                    // try to get the chest mutably
                    if let Ok((chest_ent, chest_grid_pos, mut provider_inventory)) =
                        provider_chest_query.get_mut(*from)
                    {
                        // checks if the target is at reach
                        // let current_target_tile_pos =
                        //     world_pos_to_rounded_tile(global_transform.translation().xy());
                        // let current_unit_tile_pos =
                        //     world_pos_to_rounded_tile(unit_transform.translation.xy());
                        // let distance =
                        //     tile_distance(current_target_tile_pos, current_unit_tile_pos);
                        let distance = tile_distance(*chest_grid_pos, *unit_grid_pos);
                        if distance as u8 > UNIT_REACH {
                            current_action.action = None;
                            continue;
                        }

                        let available_quantity = provider_inventory.count(kind);
                        let quantity_to_take = min(*quantity, available_quantity);

                        if quantity_to_take == 0 {
                            let reserved_by_owner =
                                reservations.owner_reserved(unit_ent, *from, *kind);
                            if reserved_by_owner > 0 {
                                reservations.release(unit_ent, *from, *kind, reserved_by_owner);
                            }
                            current_action.action = None;
                            continue;
                        }

                        provider_inventory.remove(kind, quantity_to_take);
                        unit_inventory.add(*kind, quantity_to_take);

                        let reserved_by_owner = reservations.owner_reserved(unit_ent, *from, *kind);
                        if reserved_by_owner > 0 {
                            let to_release = min(reserved_by_owner, quantity_to_take);
                            reservations.release(unit_ent, *from, *kind, to_release);
                        }
                    }
                    current_action.action = None;
                }

                Action::Drop { kind, quantity, to } => {
                    if let Ok((mut requester_inventory, grid_pos)) =
                        requester_chest_query.get_mut(*to)
                    {
                        // checks if the target is at reach
                        // let current_target_tile_pos =
                        //     world_pos_to_rounded_tile(global_transform.translation().xy());
                        // let current_unit_tile_pos =
                        //     world_pos_to_rounded_tile(unit_transform.translation.xy());
                        // let distance =
                        //     tile_distance(current_target_tile_pos, current_unit_tile_pos);
                        let distance = tile_distance(*grid_pos, *unit_grid_pos);
                        if distance as u8 > UNIT_REACH {
                            current_action.action = None;
                            continue;
                        }

                        let available_quantity = unit_inventory.count(kind);
                        let quantity_to_take = min(*quantity, available_quantity);

                        if quantity_to_take == 0 {
                            current_action.action = None;
                            continue;
                        }

                        unit_inventory.remove(kind, quantity_to_take);
                        requester_inventory.add(*kind, quantity_to_take);
                    }
                    current_action.action = None
                }

                Action::Craft {
                    recipe: _,
                    quantity: _,
                    with: _,
                } => {
                    // TODO: do that
                    todo!("process_current_action_system() : case Action::Craft");
                    current_action.action = None;
                }
            }
        }
    }
}

/// Mark tasks completed/failed and release leftovers.
/// Logic:
/// - If a unit has a Task in Planned state and both ActionQueue empty & no CurrentAction -> mark Completed and clear task + release any leftover reservations.
/// - If a Task is Failed -> release reservations & clear task (so it can be retried).
pub fn update_task_completion_system(
    mut commands: Commands,
    mut reservations: ResMut<Reservations>,
    mut unit_query: Query<
        (Entity, &ActionQueue, &mut CurrentTask, &CurrentAction),
        (With<Unit>, With<Inventory>),
    >,
) {
    for (unit_ent, action_queue, mut current_task, current_action) in unit_query.iter_mut() {
        // nothing to do
        let Some(task) = &mut current_task.task else {
            continue;
        };

        match task.status {
            TaskStatus::Planned | TaskStatus::InProgress => {
                if action_queue.0.is_empty() && current_action.action.is_none() {
                    task.status = TaskStatus::Completed;
                    reservations.release_all_for_owner(unit_ent);
                    current_task.reset();
                    match commands.get_entity(unit_ent) {
                        Ok(mut entity_command) => entity_command.insert(Available),
                        Err(_) => todo!(),
                    };
                }
            }
            TaskStatus::Failed => {
                reservations.release_all_for_owner(unit_ent);
                current_task.reset();
                match commands.get_entity(unit_ent) {
                    Ok(mut entity_command) => entity_command.insert(Available),
                    Err(_) => todo!(),
                };
            }
            TaskStatus::Pending | TaskStatus::Completed => {
                // nothing special
            }
        }
    }
}

fn find_best_chest(
    unit_tile_pos: GridPos,
    desired_quantity: u32,
    desired_item_kind: ItemKind,
    chest_query: &Query<
        (Entity, &GridPos, &Inventory),
        (With<Chest>, With<Provider>, Without<Unit>),
    >,
    reservations: &Reservations,
) -> Option<(Entity, GridPos, u32)> {
    let mut best_with_enough: Option<(Entity, GridPos, u32, f32)> = None; // (entity, tile, qty, distance)
    let mut best_any: Option<(Entity, GridPos, u32, f32)> = None; // nearest with at least 1

    for (chest_ent, chest_grid_pos, chest_inv) in chest_query.iter() {
        // let chest_tile = world_pos_to_rounded_tile(chest_global_transform.translation().xy());
        // let dist = tile_distance(unit_tile_pos, chest_tile);
        let dist = tile_distance(unit_tile_pos, *chest_grid_pos);
        let real_quantity = chest_inv.count(&desired_item_kind);
        let already_reserved = reservations.total_reserved(chest_ent, desired_item_kind);
        let available_quantity = real_quantity.saturating_sub(already_reserved);

        if available_quantity == 0 {
            continue;
        }

        if available_quantity >= desired_quantity {
            match &best_with_enough {
                None => {
                    best_with_enough = Some((chest_ent, *chest_grid_pos, available_quantity, dist))
                }
                Some((_, _, _, best_dist)) if dist < *best_dist => {
                    best_with_enough = Some((chest_ent, *chest_grid_pos, available_quantity, dist))
                }
                _ => {}
            }
        } else {
            match &best_any {
                None => best_any = Some((chest_ent, *chest_grid_pos, available_quantity, dist)),
                Some((_, _, _, best_dist)) if dist < *best_dist => {
                    best_any = Some((chest_ent, *chest_grid_pos, available_quantity, dist))
                }
                _ => {}
            }
        }
    }

    if let Some((e, pos, qty, _)) = best_with_enough {
        Some((e, pos, qty))
    } else if let Some((e, pos, qty, _)) = best_any {
        Some((e, pos, qty))
    } else {
        None
    }
}

/// Test helper: assign a GetItems task when pressing E (safe: only assign when no current task or previous task completed/failed)
fn test_find_2_rocks_system(
    mut unit_query: Query<
        &mut CurrentTask,
        (
            With<Unit>,
            With<PathfindingAgent>,
            With<Available>,
            Without<Selected>,
        ),
    >,
    mut selected_units_query: Query<
        &mut CurrentTask,
        (
            With<Unit>,
            With<PathfindingAgent>,
            With<Available>,
            With<Selected>,
        ),
    >,
) {
    let mut counter = 0;
    for mut unit_current_task in unit_query.iter_mut() {
        if counter > 0 {
            return;
        }
        // Only assign if there's no task or last task finished/failed
        let assign = match &unit_current_task.task {
            None => true,
            Some(t) => matches!(t.status, TaskStatus::Completed | TaskStatus::Failed),
        };
        if assign {
            let find_2_rocks = Task::new(
                TaskKind::GetItems {
                    kind: ItemKind::Rock,
                    quantity: 2,
                },
                Vec::new(),
            );
            unit_current_task.task = Some(find_2_rocks);
            unit_current_task.initialized = false;
        }
        counter += 1;
    }
}

/// Test helper: assign a DeliverItems task that goes to the requester chest and drops 2 rocks
fn test_deliver_2_rocks_system(
    mut unit_query: Query<&mut CurrentTask, (With<Unit>, With<PathfindingAgent>, With<Available>)>,
) {
    let mut counter = 0;
    for mut unit_current_task in unit_query.iter_mut() {
        if counter > 0 {
            return;
        }
        // assign only if idle or previous task finished/failed
        let assign = match &unit_current_task.task {
            None => true,
            Some(t) => matches!(t.status, TaskStatus::Completed | TaskStatus::Failed),
        };
        if assign {
            let deliver_2_rocks = Task::new(
                TaskKind::DeliverItems {
                    kind: ItemKind::Rock,
                    quantity: 2,
                },
                Vec::new(),
            );
            unit_current_task.task = Some(deliver_2_rocks);
            unit_current_task.initialized = false;
        }
        counter += 1;
    }
}

pub fn display_reservations_system(
    reservations: Res<Reservations>,
    unit_query: Query<&CurrentAction>,
) {
    println!("reservations: {:?}", reservations.reserved);
    for current_action in unit_query.iter() {
        if let Some(action) = &current_action.action {
            println!("current_action: {:?}", action);
        }
    }
}
