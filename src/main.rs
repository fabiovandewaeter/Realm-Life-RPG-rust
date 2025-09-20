use bevy::{
    diagnostic::FrameTimeDiagnosticsPlugin, input::common_conditions::input_pressed, prelude::*,
    time::common_conditions::on_timer,
};
use rand::{Rng, rng};
use realm_life_rpg::{
    UPS_TARGET,
    camera::{
        CameraMovement, CameraMovementKind, UpsCounter, display_fps_ups_system,
        handle_camera_inputs_system,
    },
    items::display_inventories,
    map::{CurrentMap, GridPos, MapConfig, MapId, MapPlugin, MapType, MultiMapManager, Portal},
    pathfinding::PathfindingPlugin,
    units::{
        Player, Unit, UnitUnitCollisions, UnitsPlugin,
        movements::{Direction, TileMovement},
        states::{Available, Emotions},
        tasks::{TasksPlugin, display_reservations_system},
    },
};
use std::time::Duration;
use tera::{Context, Tera};

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Overlord".to_string(),
                        present_mode: bevy::window::PresentMode::AutoVsync,
                        ..default()
                    }),
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(UnitsPlugin)
        .add_plugins(MapPlugin)
        .add_plugins(PathfindingPlugin)
        .add_plugins(TasksPlugin)
        .insert_resource(TimeState::default())
        .insert_resource(UpsCounter {
            ticks: 0,
            last_second: 0.0,
            ups: 0,
        })
        .insert_resource(Time::<Fixed>::from_hz(UPS_TARGET as f64))
        .add_systems(Startup, setup_system)
        .add_systems(
            Update,
            (
                handle_camera_inputs_system,
                display_fps_ups_system,
                control_time_system,
                generate_npc_dialogue,
            ),
        )
        .add_systems(
            FixedUpdate,
            (
                update_logic_system,
                display_inventories.run_if(input_pressed(KeyCode::KeyI)),
                display_reservations_system.run_if(on_timer(Duration::from_secs(5))),
            ),
        )
        .run();
}

#[derive(Component)]
struct PersonName(String);

#[derive(Component)]
struct Npc;

// Système qui gère l'interaction et génère la phrase.
fn generate_npc_dialogue(
    // Détecter l'appui sur la touche Espace.
    keyboard_input: Res<ButtonInput<KeyCode>>,
    // Accéder aux ressources globales.
    dialogue_engine: Res<DialogueEngine>,
    world_state: Res<WorldState>,
    // Récupérer les données des entités PNJ et Joueur.
    query_npc: Query<(&PersonName, &Emotions), With<Npc>>,
    query_player: Query<&PersonName, With<Player>>,
) {
    // On ne fait rien si la touche n'est pas pressée.
    if !keyboard_input.just_pressed(KeyCode::KeyK) {
        return;
    }

    // On récupère le premier PNJ et le premier joueur trouvés (pour cet exemple simple).
    let Ok((npc_name, npc_emotions)) = query_npc.get_single() else {
        return;
    };
    let Ok(player_name) = query_player.get_single() else {
        return;
    };

    // 1. On crée le "contexte" : c'est l'ensemble des données pour le template.
    let mut context = Context::new();
    context.insert("player_name", &player_name.0);
    context.insert("emotions", npc_emotions); // On passe toute la struct Emotions
    context.insert("weather", &world_state.weather);

    // let names = dialogue_engine.tera.get_template_names();
    // for n in names {
    //     println!("{}", n);
    // }

    // 2. On "rend" le template avec le contexte.
    match dialogue_engine.tera.render("npc_greetings.tera", &context) {
        Ok(rendered_text) => {
            // On nettoie le texte des espaces superflus.
            let final_text = rendered_text.trim().to_string();
            println!("[{}] dit : \"{}\"", npc_name.0, final_text);
        }
        Err(e) => {
            eprintln!("Erreur de rendu du dialogue : {}", e);
        }
    }
}

fn setup_system(
    mut commands: Commands,
    mut materials: ResMut<Assets<ColorMaterial>>,
    asset_server: Res<AssetServer>,
    // mut structure_manager: ResMut<StructureManager>,
    // mut chunk_manager: ResMut<ChunkManager>,
    mut multi_map_manager: ResMut<MultiMapManager>,
) {
    let tera =
        Tera::new("assets/dialogues/*.tera").expect("Erreur de chargement des templates Tera");
    commands.insert_resource(DialogueEngine { tera });
    commands.insert_resource(WorldState {
        weather: "Pluie".to_string(),
    });
    // On crée l'entité du joueur.
    commands.spawn((Player, PersonName("Aventureux".to_string())));
    commands.spawn((
        Npc,
        PersonName("Gérard le Garde".to_string()),
        Emotions { colere: 0.53 }, // 53% de colère
    ));

    let mut orthographic_projection = OrthographicProjection::default_2d();
    orthographic_projection.scale *= 0.8;
    let projection = Projection::Orthographic(orthographic_projection);
    commands.spawn((
        Camera2d,
        Camera { ..default() },
        projection,
        CameraMovement(CameraMovementKind::SmoothFollowPlayer),
        CurrentMap::default(),
    ));

    let mut rng = rng();
    let player_texture_handle = asset_server.load("default.png");
    for _i in 0..100 {
        // let random_multiplier = rng.random_range(1..=50);
        let random_multiplier = rng.random_range(5..=10);
        let random_speed = UPS_TARGET as u32 / random_multiplier;
        // let world_pos = rounded_tile_pos_to_world(GridPos { x: 0, y: 0 });

        // let mut sprite = Sprite::from_image(player_texture_handle.clone());
        let sprite = Sprite {
            image: player_texture_handle.clone(),
            // custom_size: Some(Vec2::new(32.0, 32.0)),
            ..default()
        };
        commands.spawn((
            Unit {
                name: "Unit".into(),
            },
            sprite,
            // Transform::from_translation(world_pos.extend(0.0)),
            TileMovement::new(random_speed),
            GridPos { x: 0, y: 0 },
            Available,
            UnitUnitCollisions,
            CurrentMap::default(),
        ));
    }
    // let speed = u32::MAX;
    let speed = UPS_TARGET as u32 / 5;
    // let world_pos = rounded_tile_pos_to_world(GridPos { x: 5, y: 0 });
    // uses Unit required componenents to make it easier
    commands.spawn((
        Unit {
            name: "Player".into(),
        },
        Sprite::from_image(player_texture_handle.clone()),
        // Transform::from_translation(world_pos.extend(0.0)),
        GridPos { x: 5, y: 0 },
        TileMovement::new(speed),
        UnitUnitCollisions,
        CurrentMap::default(),
        // Player,
    ));
    // CRÉER UNE MAISON AVEC PORTAILS
    println!("Creating house...");

    // 1. Créer la config de la maison
    let house_config = MapConfig {
        id: MapId(0), // Sera remplacé
        name: "House Interior".to_string(),
        map_type: MapType::House { owner_id: None },
        size: UVec2::new(10, 10),
        spawn_point: GridPos { x: 5, y: 1 },
    };

    let house_map_id = multi_map_manager.create_map(house_config);
    println!("Created house map with ID: {:?}", house_map_id);

    // Portail d'entrée (regarde vers le nord)
    let entrance_portal = commands
        .spawn((
            Portal {
                target_map: house_map_id,
                target_position: GridPos { x: 5, y: 1 },
                facing_direction: Direction::North,
            },
            GridPos { x: 10, y: 10 },
            CurrentMap { map_id: MapId(0) },
            Sprite::from_image(asset_server.load("structures/door.png")),
        ))
        .id();

    // Portail de sortie (regarde vers le sud)
    let exit_portal = commands
        .spawn((
            Portal {
                target_map: MapId(0),
                target_position: GridPos { x: 10, y: 9 },
                facing_direction: Direction::South,
            },
            GridPos { x: 5, y: 0 },
            CurrentMap {
                map_id: house_map_id,
            },
            Sprite::from_image(asset_server.load("structures/door.png")),
        ))
        .id();
}

pub fn update_logic_system(mut counter: ResMut<UpsCounter>) {
    counter.ticks += 1;
}

#[derive(Resource, Default)]
struct TimeState {
    is_paused: bool,
}

fn control_time_system(
    mut fixed_time: ResMut<Time<Fixed>>,
    input: Res<ButtonInput<KeyCode>>,
    mut time_state: ResMut<TimeState>,
) {
    // P pour Pause, pour alterner entre l'état de pause
    if input.just_pressed(KeyCode::Space) {
        if time_state.is_paused {
            println!("Temps de la simulation repris.");
            fixed_time.set_timestep_hz(UPS_TARGET as f64);
            time_state.is_paused = false;
        } else {
            println!("Temps de la simulation mis en pause.");
            fixed_time.set_timestep_hz(0.0);
            time_state.is_paused = true;
        }
    }

    // Si le jeu est en pause, on ne gère pas les autres commandes de vitesse
    if time_state.is_paused {
        return;
    }

    // Accélérer (x2)
    if input.just_pressed(KeyCode::KeyY) {
        let current_hz = fixed_time.timestep().as_secs_f64().recip();
        let new_hz = current_hz * 2.0;
        println!("Temps de la simulation accéléré à {} Hz.", new_hz);
        fixed_time.set_timestep_hz(new_hz);
    }

    // Ralentir (/2)
    if input.just_pressed(KeyCode::KeyU) {
        let current_hz = fixed_time.timestep().as_secs_f64().recip();
        let new_hz = current_hz / 2.0;
        println!("Temps de la simulation ralenti à {} Hz.", new_hz);
        fixed_time.set_timestep_hz(new_hz);
    }

    // Normal (retour à la vitesse initiale)
    if input.just_pressed(KeyCode::KeyI) {
        println!("Temps de la simulation réinitialisé à {} Hz.", UPS_TARGET);
        fixed_time.set_timestep_hz(UPS_TARGET as f64);
    }
}

#[derive(Resource)]
struct WorldState {
    weather: String,
}

#[derive(Resource)]
struct DialogueEngine {
    tera: Tera,
}
