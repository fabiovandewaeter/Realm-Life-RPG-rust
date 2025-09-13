use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Component)]
pub struct Item;

/// for stackable items
#[derive(Component, Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ItemKind {
    Rock,
}

/// can't be stacked in the code but can be showed as stacked in the game UI
#[derive(Component, Clone, Copy, Debug)]
pub enum UniqueItemKind {
    IronSword,
}

#[derive(Component, Default, Debug)]
pub struct Inventory {
    pub stackable_items: HashMap<ItemKind, u32>,
    pub unique_items: Vec<Entity>,
    // pub unique_items: HashMap<UniqueItemKind, Vec<Entity>>,
}

#[derive(Component)]
pub struct Durability {
    pub current: u16,
    pub max: u16,
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum CraftRecipeId {
    Chest,
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            stackable_items: HashMap::new(),
            unique_items: Vec::new(),
        }
    }

    pub fn add(&mut self, kind: ItemKind, quantity: u32) {
        *self.stackable_items.entry(kind).or_insert(0) += quantity;
    }

    /// remove up to quantity, returns true if fully removed, false if not enough
    pub fn remove(&mut self, kind: &ItemKind, quantity: u32) -> bool {
        if let Some(current_quantity) = self.stackable_items.get_mut(kind) {
            if *current_quantity >= quantity {
                *current_quantity -= quantity;
                if *current_quantity == 0 {
                    self.stackable_items.remove(kind);
                }
                return true;
            }
        }
        false
    }

    pub fn count(&self, kind: &ItemKind) -> u32 {
        *self.stackable_items.get(kind).unwrap_or(&0)
    }

    pub fn add_unique_item(&mut self, item_entity: Entity) {
        self.unique_items.push(item_entity);
    }

    pub fn remove_unique_item(&mut self, item_entity: Entity) -> bool {
        if let Some(pos) = self.unique_items.iter().position(|&x| x == item_entity) {
            self.unique_items.remove(pos);
            true
        } else {
            false
        }
    }
}

// Système d'affichage de l'inventaire
pub fn display_inventories(inventories: Query<&Inventory>) {
    if let Ok(inventory) = inventories.single() {
        println!("=== INVENTORY ===");
        for (item_kind, quantity) in &inventory.stackable_items {
            println!("{:?}: {}", item_kind, quantity);
        }
        println!("Unique items: {}", inventory.unique_items.len());
    }
}
