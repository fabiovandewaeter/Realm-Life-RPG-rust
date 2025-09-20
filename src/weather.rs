use bevy::ecs::resource::Resource;

use crate::{DAY_DURATION, UPS_TARGET};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WeatherKind {
    Clear,
    Cloudy,
    Rain,
    Snow,
    Storm,
    Fog,
}

#[derive(Resource)]
struct Climate {
    season: Season,
    temperature_c: f32, // moyenne locale
    humidity: f32,      // 0.0 - 1.0
}

#[derive(Resource)]
struct Weather {
    kind: WeatherKind,
    intensity: f32, // 0.0 - 1.0
    ticks_since_last_transition: u32,
}

impl Default for Weather {
    fn default() -> Self {
        Weather {
            kind: WeatherKind::Clear,
            intensity: 0.0,
            ticks_since_last_transition: DAY_DURATION,
        }
    }
}
