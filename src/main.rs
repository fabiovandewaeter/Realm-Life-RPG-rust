use std::time::Duration;

use bevy::{
    app::ScheduleRunnerPlugin,
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    input::{
        common_conditions::input_pressed,
        mouse::{MouseScrollUnit, MouseWheel},
    },
    prelude::*, // contient ImagePlugin, Camera2d, etc.
    // note: on retire asset::AssetServerSettings et image::ImagePlugin du chemin racine
    time::common_conditions::on_timer,
    winit::WinitPlugin,
};
use bevy_ratatui::kitty::KittyEnabled;
use bevy_ratatui::{RatatuiContext, RatatuiPlugins};
use bevy_ratatui_camera::{
    EdgeCharacters, RatatuiCamera, RatatuiCameraEdgeDetection, RatatuiCameraPlugin,
    RatatuiCameraStrategy, RatatuiCameraWidget,
};
// pour que `camera_widget.render(...)` (et autres widgets) soient reconnus
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::prelude::Widget;
use ratatui::widgets::{Block, Borders, Paragraph};

use realm_life_rpg::{
    CAMERA_SPEED, UPS_TARGET, ZOOM_IN_SPEED, ZOOM_OUT_SPEED,
    camera::{
        CameraMovement, CameraMovementKind, UpsCounter, display_fps_ups_system,
        handle_camera_inputs_system,
    },
    items::{Inventory, ItemKind, display_inventories},
    map::{
        Chest, ChunkManager, Crafter, GridPos, MapPlugin, Provider, Requester, Structure,
        StructureManager, TILE_SIZE, place_structure,
    },
    pathfinding::PathfindingPlugin,
    units::{
        Player, Unit, UnitUnitCollisions, UnitsPlugin,
        movements::TileMovement,
        states::Available,
        tasks::{TasksPlugin, display_reservations_system},
    },
};

use rand::{Rng, rng};

fn snap_camera_to_cell_grid(mut q: Query<(&mut Transform, &Projection), With<RatatuiCamera>>) {
    for (mut transform, projection) in q.iter_mut() {
        if let Projection::Orthographic(p) = projection {
            let s = p.scale; // zoom
            // on arrondit la position multipliée par le zoom, puis on remet
            transform.translation.x = (transform.translation.x * s).round() / s;
            transform.translation.y = (transform.translation.y * s).round() / s;
        }
    }
}

fn main() {
    App::new()
        // Plugins : désactiver la fenetre native (Winit) et utiliser le ScheduleRunner pour loop
        .add_plugins((
            // Partiellement configurer DefaultPlugins :
            DefaultPlugins
                .build()
                .disable::<WinitPlugin>() // plus de fenêtre native
                .set(ImagePlugin::default_nearest()), // conserve nearest filtering pour images si utiles
            // Boucle principale "manuelle" : cadence de rendu (ici ~60 FPS)
            ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0)),
            // Diagnostics (Fps, etc)
            FrameTimeDiagnosticsPlugin::default(),
            // Terminal UI + camera plugin
            // RatatuiPlugins::default(),
            RatatuiPlugins {
                enable_input_forwarding: true,
                ..default()
            },
            RatatuiCameraPlugin,
        ))
        // Tes plugins de jeu inchangés
        .add_plugins(UnitsPlugin)
        .add_plugins(MapPlugin)
        .add_plugins(PathfindingPlugin)
        .add_plugins(TasksPlugin)
        // Ressources / Time
        .insert_resource(TimeState::default())
        .insert_resource(UpsCounter {
            ticks: 0,
            last_second: 0.0,
            ups: 0,
        })
        .insert_resource(Time::<Fixed>::from_hz(UPS_TARGET))
        // Systèmes
        .add_systems(Startup, setup_system)
        .add_systems(
            Update,
            (
                // snap_camera_to_cell_grid,
                handle_camera_inputs_system,
                // display_fps_ups_system,
                control_time_system,
            ),
        )
        .add_systems(
            FixedUpdate,
            (
                update_logic_system,
                display_inventories
                    .run_if(bevy::input::common_conditions::input_pressed(KeyCode::KeyI)),
                // display_reservations_system.run_if(bevy::time::common_conditions::on_timer(
                //     Duration::from_secs(5),)),
            ),
        )
        // draw_system doit tourner en Update pour appeler ratatui.draw()
        .add_systems(Update, draw_system)
        .run();
}

fn setup_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    asset_server: Res<AssetServer>,
    mut structure_manager: ResMut<StructureManager>,
    mut chunk_manager: ResMut<ChunkManager>,
) {
    // Ta caméra initiale : on ajoute aussi RatatuiCamera + strategy
    let mut orthographic_projection = OrthographicProjection::default_2d();
    orthographic_projection.scale *= 0.8;
    let projection = Projection::Orthographic(orthographic_projection);

    commands.spawn((
        Camera2d,
        Camera { ..default() },
        projection,
        CameraMovement(CameraMovementKind::FreeCamera),
        // core : le composant qui permet à bevy_ratatui_camera d'extraire le rendu
        RatatuiCamera::default(),
        // stratégie de rendu : luminance avec braille -> bonne densité pour scènes
        // RatatuiCameraStrategy::luminance_braille(),
        // RatatuiCameraStrategy::luminance_blocks(),
        RatatuiCameraEdgeDetection {
            thickness: 1.2, // expérimente 0.8..2.0
            edge_characters: EdgeCharacters::Directional {
                vertical: '|',
                horizontal: '─',
                forward_diagonal: '/',
                backward_diagonal: '\\',
            },
            edge_color: Some(ratatui::prelude::Color::White),
            ..Default::default()
        },
        RatatuiCameraStrategy::halfblocks(),
    ));

    // Exemple: spawn d'une mesh et des units comme auparavant
    commands.spawn((
        Mesh2d(meshes.add(Rectangle::new(20.0, 20.0))),
        MeshMaterial2d(materials.add(Color::from(bevy::color::palettes::css::GREEN))),
    ));

    let mut rng = rng();
    let player_texture_handle = asset_server.load("default.png");
    for _i in 0..100 {
        let random_multiplier = rng.random_range(5..=10);
        let random_speed = UPS_TARGET as u32 / random_multiplier;

        let sprite = Sprite {
            image: player_texture_handle.clone(),
            ..default()
        };
        commands.spawn((
            Unit {
                name: "Unit".into(),
            },
            sprite,
            TileMovement::new(random_speed),
            GridPos { x: 0, y: 0 },
            Available,
            UnitUnitCollisions,
        ));
    }

    let speed = UPS_TARGET as u32 / 5;
    commands.spawn((
        Unit {
            name: "Player".into(),
        },
        Sprite::from_image(player_texture_handle.clone()),
        GridPos { x: 5, y: 0 },
        TileMovement::new(speed),
        UnitUnitCollisions,
    ));

    // ... spawn chests / crafter comme dans ton code (abrégé ici)
    // (je laisse inchangé l'appel à place_structure etc.)
    // exemple pour un coffre:
    let mut inventory = Inventory::new();
    inventory.add(ItemKind::Rock, 1000);
    let chest_entity = commands
        .spawn((
            Structure,
            Chest,
            Sprite::from_image(asset_server.load("structures/chest.png")),
            inventory,
            Provider,
        ))
        .id();
    let rounded_tile_pos = GridPos { x: 10, y: 5 };
    place_structure(
        &mut commands,
        &asset_server,
        &chest_entity,
        &mut structure_manager,
        &mut chunk_manager,
        rounded_tile_pos,
    );

    // répéter pour les autres structures...
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
    if input.just_pressed(KeyCode::Space) {
        if time_state.is_paused {
            println!("Temps de la simulation repris.");
            fixed_time.set_timestep_hz(UPS_TARGET);
            time_state.is_paused = false;
        } else {
            println!("Temps de la simulation mis en pause.");
            fixed_time.set_timestep_hz(0.0);
            time_state.is_paused = true;
        }
    }

    if time_state.is_paused {
        return;
    }

    if input.just_pressed(KeyCode::KeyY) {
        let current_hz = fixed_time.timestep().as_secs_f64().recip();
        let new_hz = current_hz * 2.0;
        println!("Temps de la simulation accéléré à {} Hz.", new_hz);
        fixed_time.set_timestep_hz(new_hz);
    }

    if input.just_pressed(KeyCode::KeyU) {
        let current_hz = fixed_time.timestep().as_secs_f64().recip();
        let new_hz = current_hz / 2.0;
        println!("Temps de la simulation ralenti à {} Hz.", new_hz);
        fixed_time.set_timestep_hz(new_hz);
    }

    if input.just_pressed(KeyCode::KeyI) {
        println!("Temps de la simulation réinitialisé à {} Hz.", UPS_TARGET);
        fixed_time.set_timestep_hz(UPS_TARGET);
    }
}

// ===== draw system: split terminal UI (gauche texte, droite rendu caméra) =====
fn draw_system(
    mut ratatui: ResMut<RatatuiContext>,
    mut camera_widget: Single<&mut RatatuiCameraWidget>,
    diagnostics: Res<DiagnosticsStore>,
    kitty_enabled: Option<Res<KittyEnabled>>,
    ups: Res<UpsCounter>,
) {
    ratatui.draw(|frame| {
        // on utilise la frame entière ; tu peux adapter pour ajouter bordures/debug
        let size = frame.size();

        // découpe horizontale : gauche 35% = ratatui widget, droite 65% = camera
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)].as_ref())
            .split(size);

        // Widget gauche : exemple simple (infos)
        let left_text = format!(
            "Overlord (terminal)\n\nFPS/UPS : (voir diagnostics)\nUPS ticks: {}\n\nTouches:\n  Space : pause/play\n  Y/U/I : vitesse\n",
            ups.ticks
        );
        let paragraph = Paragraph::new(left_text).block(Block::default().title("Info").borders(Borders::ALL));
        frame.render_widget(paragraph, chunks[0]);

        // Widget droit : rendu de la caméra
        camera_widget.render(chunks[1], frame.buffer_mut());

        // IMPORTANT : la closure doit renvoyer () (pas Result)
    });
}
