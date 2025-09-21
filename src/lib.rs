pub mod camera;
pub mod items;
pub mod map;
pub mod pathfinding;
pub mod ui;
pub mod units;
pub mod weather;

pub const UPS_TARGET: u32 = 30; // 30 ticks per second
pub const ZOOM_IN_SPEED: f32 = 0.25 / 400000000.0;
pub const ZOOM_OUT_SPEED: f32 = 4.0 * 400000000.0;
pub const CAMERA_SPEED: f32 = 37.5;

pub const DAY_DURATION: u32 = UPS_TARGET * 60 * 10; // 10 minutes in ticks
