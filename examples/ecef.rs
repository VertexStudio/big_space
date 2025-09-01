//! Demonstrates Earth-Centered, Earth-Fixed (ECEF) positions with big_space.
//!
//! Requires features: `camera`
//! Run: `cargo run --release --example ecef --features "camera"`
//!
//! ECEF uses a right-handed Cartesian frame with origin at Earth's center.
//! We map ECEF Z -> Bevy Y (north/up) and provide a "Google Earth"-style
//! orbit controller: the camera orbits Earth, zooms in/out, and always keeps
//! north-up (no roll), while leveraging big_space's floating origin.

use bevy::prelude::*;
use bevy_input::mouse::{MouseMotion, MouseScrollUnit, MouseWheel};
use bevy_math::DVec3;
use bevy_transform::TransformSystem;
use big_space::prelude::*;

const EARTH_RADIUS_M: f64 = 6_371_000.0; // mean radius (m)
const CELL_SIZE_M: f64 = 10_000.0; // 10 km cells
const DEFAULT_ORBIT_DISTANCE_M: f64 = EARTH_RADIUS_M * 2.5; // start several radii out
const DEFAULT_ORBIT_YAW_RAD: f64 = 0.0; // face prime meridian
const DEFAULT_ORBIT_PITCH_RAD: f64 = 0.7; // ~40° above the equator
const CAMERA_FOV_DEGREES: f32 = 70.0;
const CAMERA_NEAR_METERS: f32 = 1.0;
const CAMERA_FAR_PAD: f64 = 1.1; // Overdraw buffer beyond zoom limit
const DEBUG_LOGS: bool = false; // Set true to enable verbose logging systems

fn main() {
    let mut app = App::new();
    app.add_plugins((
        // Disable Bevy's transform plugin; big_space provides its own propagation.
        DefaultPlugins.build().disable::<TransformPlugin>(),
        BigSpaceDefaultPlugins,
    ));
    app.insert_resource(ClearColor(Color::srgb(0.05, 0.05, 0.1)));
    app.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 5000.0,
        ..default()
    });
    app.add_systems(Startup, setup);
    // Core orbit and gizmos
    app.add_systems(
        PostUpdate,
        (
            orbit_camera_update.before(TransformSystem::TransformPropagate),
            draw_earth_gizmos.after(TransformSystem::TransformPropagate),
        ),
    );
    // Optional verbose debug systems
    if DEBUG_LOGS {
        app.add_systems(
            PostUpdate,
            (
                debug_camera_position.after(orbit_camera_update),
                check_rendering_setup.after(TransformSystem::TransformPropagate),
            ),
        );
    }
    app.run();
}

/// Build the Earth scene, meshes, and orbiting camera used in the demo.
fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if DEBUG_LOGS {
        println!("=== ECEF Setup Starting ===");
    }

    // Planet material and mesh - unit sphere, scaled via transform for precision
    let earth_mesh = meshes.add(Sphere::new(1.0).mesh().ico(32).unwrap());

    // Load Earth texture
    let earth_texture = asset_server.load("textures/world.topo.bathy.200412.3x5400x2700.jpg");
    let earth_mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(earth_texture),
        unlit: true, // Render texture as-is, unaffected by lights
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // Also create debug mesh as unit sphere, scale via transform
    let debug_mesh = meshes.add(Sphere::new(1.0).mesh().ico(3).unwrap());
    let debug_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.0, 0.0),   // Red
        emissive: LinearRgba::rgb(1.0, 0.0, 0.0), // Make it glow
        ..default()
    });

    // Create the floating-origin hierarchy that the rest of the example hangs off of.
    commands.spawn_big_space(Grid::new(CELL_SIZE_M as f32, 0.0), |root| {
        // Spawn Earth at origin
        root.spawn_spatial((
            Name::new("Earth"),
            Mesh3d(earth_mesh.clone()),
            MeshMaterial3d(earth_mat),
            Transform::from_scale(Vec3::splat(EARTH_RADIUS_M as f32)),
            CellCoord::default(),
        ));

        // Debug sphere
        root.spawn_spatial((
            Name::new("DebugSphere"),
            Mesh3d(debug_mesh),
            MeshMaterial3d(debug_mat),
            Transform::from_scale(Vec3::splat(EARTH_RADIUS_M as f32 * 0.5)),
            CellCoord::default(),
        ));

        // Camera with floating origin so rendering follows the viewer.
        let fov = CAMERA_FOV_DEGREES.to_radians();

        let camera_orbit = OrbitState::default();
        let start_yaw = camera_orbit.yaw;
        let start_pitch = camera_orbit.pitch;
        let start_distance = camera_orbit.distance;

        // Initial position
        let cos_pitch = start_pitch.cos();
        let sin_pitch = start_pitch.sin();
        let cos_yaw = start_yaw.cos();
        let sin_yaw = start_yaw.sin();
        let camera_world_position = DVec3::new(
            cos_pitch * cos_yaw * start_distance,
            sin_pitch * start_distance,
            cos_pitch * sin_yaw * start_distance,
        );
        let (camera_cell, camera_local_offset) =
            root.grid().translation_to_grid(camera_world_position);
        let camera_far = ((camera_orbit.zoom_limit_max + EARTH_RADIUS_M) * CAMERA_FAR_PAD) as f32;

        root.spawn_spatial((
            Name::new("Camera"),
            Camera3d::default(),
            Projection::Perspective(PerspectiveProjection {
                fov,
                near: CAMERA_NEAR_METERS,
                far: camera_far,
                ..default()
            }),
            Transform::from_translation(camera_local_offset).looking_at(Vec3::ZERO, Vec3::Y),
            camera_cell,
            FloatingOrigin, // THIS IS KEY
            camera_orbit,
        ))
        .with_children(|parent| {
            parent.spawn((
                DirectionalLight {
                    illuminance: 50_000.0,
                    ..default()
                },
                Transform::default(),
            ));
        });
    });

    if DEBUG_LOGS {
        println!("=== ECEF Setup Complete ===");
        println!("Controls:");
        println!("  Left-click + drag: Orbit");
        println!("  Mouse wheel/scroll: Zoom in/out");
        println!("  R key: Reset camera position");
    }
}

/// Captures both the current and target state of the orbit controller.
#[derive(Component, Debug, Clone)]
struct OrbitState {
    distance: f64,
    yaw: f64,   // Orbit angle around Y axis (longitude)
    pitch: f64, // Orbit angle from equator (latitude)
    // Desired targets (for smooth, critically damped motion)
    desired_distance: f64,
    desired_yaw: f64,
    desired_pitch: f64,
    // Controls
    invert_y: bool,
    yaw_sensitivity: f64,
    pitch_sensitivity: f64,
    zoom_sensitivity: f64,
    // Physics-like smoothing
    yaw_vel: f64,
    pitch_vel: f64,
    dist_vel: f64,
    angular_smooth_time: f64, // seconds to reach target (~critically damped)
    zoom_smooth_time: f64,    // seconds to reach target (~critically damped)
    max_angular_speed: f64,   // rad/s cap (for SmoothDamp)
    max_zoom_speed: f64,      // m/s cap (for SmoothDamp)
    // Pole protection
    pole_soft_deg: f64,  // start rubberband at this absolute pitch (deg)
    pole_limit_deg: f64, // hard stop (deg) to avoid singularity
    // Zoom protection (soft rubberband + hard clamp)
    zoom_soft_min: f64,
    zoom_limit_min: f64,
    zoom_soft_max: f64,
    zoom_limit_max: f64,
    // Rubber band compression factor (larger = stiffer beyond soft bound)
    rubber_k: f64,
}

impl OrbitState {
    /// Construct a controller state focused on the given distance and angles.
    fn new(distance: f64, yaw: f64, pitch: f64) -> Self {
        Self {
            distance,
            yaw,
            pitch,
            desired_distance: distance,
            desired_yaw: yaw,
            desired_pitch: pitch,
            invert_y: false,
            yaw_sensitivity: 0.005,
            pitch_sensitivity: 0.005,
            zoom_sensitivity: 0.1,
            yaw_vel: 0.0,
            pitch_vel: 0.0,
            dist_vel: 0.0,
            angular_smooth_time: 0.10,
            zoom_smooth_time: 0.12,
            max_angular_speed: std::f64::INFINITY,
            max_zoom_speed: std::f64::INFINITY,
            pole_soft_deg: 85.0,
            pole_limit_deg: 89.0,
            zoom_soft_min: EARTH_RADIUS_M * 1.03,
            zoom_limit_min: EARTH_RADIUS_M * 1.02,
            zoom_soft_max: EARTH_RADIUS_M * 80.0,
            zoom_limit_max: EARTH_RADIUS_M * 100.0,
            rubber_k: 0.002,
        }
    }
}

impl Default for OrbitState {
    fn default() -> Self {
        Self::new(
            DEFAULT_ORBIT_DISTANCE_M,
            DEFAULT_ORBIT_YAW_RAD,
            DEFAULT_ORBIT_PITCH_RAD,
        )
    }
}

/// Critically-damped smoothing toward a target (Unity-style SmoothDamp).
fn smooth_damp(
    current: f64,
    target: f64,
    velocity: &mut f64,
    smooth_time: f64,
    max_speed: f64,
    dt: f64,
) -> f64 {
    let smooth_time = smooth_time.max(1e-4);
    let omega = 2.0 / smooth_time;
    let x = omega * dt;
    let exp = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);

    let mut change = current - target;
    let original_to = target;
    // Clamp maximum speed
    let max_change = max_speed * smooth_time;
    if change > max_change {
        change = max_change;
    }
    if change < -max_change {
        change = -max_change;
    }
    let target = current - change;
    let temp = (*velocity + omega * change) * dt;
    *velocity = (*velocity - omega * temp) * exp;
    let mut output = target + (change + temp) * exp;

    // Prevent overshoot
    if (original_to - current > 0.0) == (output > original_to) {
        output = original_to;
        *velocity = 0.0;
    }
    output
}

/// Non-linear compression so the camera eases into soft limits without snapping.
fn rubber_compress(over: f64, k: f64) -> f64 {
    over / (1.0 + over * k)
}

/// Apply mouse/keyboard input to the orbit camera while keeping it in grid space.
fn orbit_camera_update(
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    time: Res<Time>,
    mut cameras: Query<(Entity, &mut Transform, &mut CellCoord, &mut OrbitState), With<Camera>>,
    grids: Grids,
) {
    let Ok((entity, mut transform, mut cell, mut orbit)) = cameras.single_mut() else {
        return;
    };

    let Some(grid) = grids.parent_grid(entity) else {
        return;
    };

    // Reset camera with R key
    if keys.just_pressed(KeyCode::KeyR) {
        if DEBUG_LOGS {
            println!("Resetting camera to initial position");
        }
        *orbit = OrbitState::default();
    }
    let delta_seconds = time.delta_secs_f64();

    // Left-click drag to orbit
    if buttons.pressed(MouseButton::Left) {
        if let Some(delta) = mouse_motion
            .read()
            .map(|e| e.delta)
            .reduce(|acc, d| acc + d)
        {
            let dy = if orbit.invert_y { -delta.y } else { delta.y };
            orbit.desired_yaw += delta.x as f64 * orbit.yaw_sensitivity;
            orbit.desired_pitch += dy as f64 * orbit.pitch_sensitivity;
        }
    } else {
        mouse_motion.clear();
    }

    // Scroll to zoom
    let mut scroll_delta_sum = 0.0f32;
    for event in mouse_wheel.read() {
        let delta = match event.unit {
            MouseScrollUnit::Line => event.y,
            MouseScrollUnit::Pixel => event.y / 120.0,
        };
        scroll_delta_sum += delta;
    }
    let is_zooming = scroll_delta_sum != 0.0;
    if is_zooming {
        // Positive scroll reduces distance (zoom in)
        let factor = 1.0 - (scroll_delta_sum as f64) * orbit.zoom_sensitivity;
        orbit.desired_distance = (orbit.desired_distance * factor).max(0.0);
    }

    // Targets and rubber-banding for pitch (orbit), with non-oscillatory smoothing
    let soft_pitch_limit = orbit.pole_soft_deg.to_radians();
    let hard_pitch_limit = orbit.pole_limit_deg.to_radians();
    let mut pitch_target = orbit.desired_pitch;
    if buttons.pressed(MouseButton::Left) {
        // Apply rubber compression when dragging beyond soft bound
        let abs = pitch_target.abs();
        if abs > soft_pitch_limit {
            let sign = pitch_target.signum();
            let over = abs - soft_pitch_limit;
            let compressed = rubber_compress(over, orbit.rubber_k);
            pitch_target = sign * (soft_pitch_limit + compressed);
        }
    } else {
        // Recoil to soft bound when released
        pitch_target = pitch_target.clamp(-soft_pitch_limit, soft_pitch_limit);
        orbit.desired_pitch = pitch_target;
    }
    pitch_target = pitch_target.clamp(-hard_pitch_limit, hard_pitch_limit);
    let angular_smooth_time = orbit.angular_smooth_time;
    let max_angular_speed = orbit.max_angular_speed;
    orbit.pitch = smooth_damp(
        orbit.pitch,
        pitch_target,
        &mut orbit.pitch_vel,
        angular_smooth_time,
        max_angular_speed,
        delta_seconds,
    );

    // Yaw smoothing (no limits)
    let angular_smooth_time = orbit.angular_smooth_time;
    let max_angular_speed = orbit.max_angular_speed;
    orbit.yaw = smooth_damp(
        orbit.yaw,
        orbit.desired_yaw,
        &mut orbit.yaw_vel,
        angular_smooth_time,
        max_angular_speed,
        delta_seconds,
    );

    // Zoom smoothing with rubber and recoil
    let mut distance_target = orbit.desired_distance;
    if is_zooming {
        if distance_target < orbit.zoom_soft_min {
            let over = orbit.zoom_soft_min - distance_target;
            let compressed = rubber_compress(over, orbit.rubber_k / orbit.zoom_soft_min);
            distance_target = orbit.zoom_soft_min - compressed;
        } else if distance_target > orbit.zoom_soft_max {
            let over = distance_target - orbit.zoom_soft_max;
            let compressed = rubber_compress(over, orbit.rubber_k / orbit.zoom_soft_max);
            distance_target = orbit.zoom_soft_max + compressed;
        }
    } else {
        distance_target = distance_target.clamp(orbit.zoom_soft_min, orbit.zoom_soft_max);
        orbit.desired_distance = distance_target;
    }
    distance_target = distance_target.clamp(orbit.zoom_limit_min, orbit.zoom_limit_max);
    let zoom_smooth_time = orbit.zoom_smooth_time;
    let max_zoom_speed = orbit.max_zoom_speed;
    orbit.distance = smooth_damp(
        orbit.distance,
        distance_target,
        &mut orbit.dist_vel,
        zoom_smooth_time,
        max_zoom_speed,
        delta_seconds,
    );

    // Calculate orbit position
    let cos_pitch = orbit.pitch.cos();
    let sin_pitch = orbit.pitch.sin();
    let cos_yaw = orbit.yaw.cos();
    let sin_yaw = orbit.yaw.sin();

    let camera_world_position = DVec3::new(
        cos_pitch * cos_yaw * orbit.distance,
        sin_pitch * orbit.distance,
        cos_pitch * sin_yaw * orbit.distance,
    );

    // Convert to grid coordinates
    let (cell_offset, local_pos) = grid.translation_to_grid(camera_world_position);

    // Set cell and transform
    *cell = cell_offset;
    transform.translation = local_pos;

    // Look at Earth (toward world origin). `cell + local` is the camera's
    // world position; the vector from camera to Earth's center is its negation.
    let vector_to_earth = -(cell.as_dvec3(&grid) + local_pos.as_dvec3());
    // Use the direction toward Earth directly and a fixed world-up to avoid surprises.
    let look_direction = vector_to_earth.normalize().as_vec3();
    transform.look_to(look_direction, Vec3::Y);
}

/// Render helpful gizmos that explain orientation relative to Earth.
fn draw_earth_gizmos(
    mut gizmos: Gizmos,
    q_earth: Query<(&GlobalTransform, &Name)>,
    q_cam: Query<&GlobalTransform, With<Camera>>,
) {
    // Find Earth's global transform
    let mut earth_gt = None;
    for (gt, name) in q_earth.iter() {
        if name.as_str() == "Earth" {
            earth_gt = Some(gt);
            break;
        }
    }

    if let Some(earth) = earth_gt {
        let earth_pos = earth.translation();
        // Earth mesh is already at full size, not scaled via transform
        let earth_scale = EARTH_RADIUS_M as f32;

        // Draw coordinate axes at Earth center
        let axis_length = earth_scale * 1.5;
        gizmos.line(
            earth_pos,
            earth_pos + Vec3::X * axis_length,
            Color::srgb(1.0, 0.0, 0.0),
        );
        gizmos.line(
            earth_pos,
            earth_pos + Vec3::Y * axis_length,
            Color::srgb(0.0, 1.0, 0.0),
        ); // North pole
        gizmos.line(
            earth_pos,
            earth_pos + Vec3::Z * axis_length,
            Color::srgb(0.0, 0.0, 1.0),
        );

        // Draw equator circle (in XZ plane)
        gizmos.circle(
            Isometry3d::new(earth_pos, Quat::IDENTITY),
            earth_scale,
            Color::srgb(1.0, 1.0, 0.0),
        );

        // Draw meridian circles
        gizmos.circle(
            Isometry3d::new(
                earth_pos,
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ),
            earth_scale,
            Color::srgb(0.0, 1.0, 1.0),
        );
        gizmos.circle(
            Isometry3d::new(
                earth_pos,
                Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            ),
            earth_scale,
            Color::srgb(1.0, 0.0, 1.0),
        );

        // Draw poles
        let north_pole = earth_pos + Vec3::Y * earth_scale;
        let south_pole = earth_pos - Vec3::Y * earth_scale;
        gizmos.sphere(
            Isometry3d::from_translation(north_pole),
            earth_scale * 0.05,
            Color::srgb(0.0, 1.0, 0.0),
        );
        gizmos.sphere(
            Isometry3d::from_translation(south_pole),
            earth_scale * 0.05,
            Color::srgb(1.0, 0.0, 0.0),
        );

        // Draw line from camera to Earth center
        if let Ok(cam_gt) = q_cam.single() {
            let cam_pos = cam_gt.translation();
            gizmos.line(cam_pos, earth_pos, Color::srgb(1.0, 1.0, 1.0));

            // Draw camera frustum indicator
            let cam_forward = cam_gt.forward();
            gizmos.arrow(
                cam_pos,
                cam_pos + cam_forward * earth_scale * 0.5,
                Color::srgb(1.0, 0.5, 0.0),
            );
        }

        // Draw a wireframe sphere approximation
        let segments = 16;
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let next_angle = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;

            // Horizontal circles at different latitudes
            for j in 1..6 {
                let lat = (j as f32 / 6.0 - 0.5) * std::f32::consts::PI;
                let radius = earth_scale * lat.cos();
                let y = earth_scale * lat.sin();

                let p1 = earth_pos + Vec3::new(radius * angle.cos(), y, radius * angle.sin());
                let p2 =
                    earth_pos + Vec3::new(radius * next_angle.cos(), y, radius * next_angle.sin());
                gizmos.line(p1, p2, Color::srgba(0.5, 0.5, 1.0, 0.3));
            }
        }
    } else {
        // If we can't find Earth, draw a reference grid at origin
        gizmos.grid(
            Isometry3d::IDENTITY,
            UVec2::splat(10),
            Vec2::splat(EARTH_RADIUS_M as f32 * 0.2),
            Color::srgb(0.5, 0.5, 0.5),
        );
    }
}

/// Periodically print the camera and mesh setup to the console when debugging.
fn check_rendering_setup(
    q_cam: Query<(&Camera, &GlobalTransform, &Projection), With<FloatingOrigin>>,
    q_meshes: Query<(&GlobalTransform, &Mesh3d, &Name)>,
    mut frame_count: Local<u32>,
) {
    *frame_count += 1;
    if *frame_count != 1 && *frame_count % 300 != 0 {
        return; // Check once at start and every 5 seconds
    }

    println!("\n=== RENDERING CHECK ===");

    // Check camera setup
    if let Ok((camera, cam_gt, projection)) = q_cam.single() {
        println!("Camera Rendering Info:");
        println!("  Is active: {}", camera.is_active);
        println!(
            "  Camera at: {:.1}, {:.1}, {:.1}",
            cam_gt.translation().x,
            cam_gt.translation().y,
            cam_gt.translation().z
        );

        if let Projection::Perspective(persp) = projection {
            println!("  FOV: {:.1}°", persp.fov.to_degrees());
            println!("  Near: {:.1}, Far: {:.1}", persp.near, persp.far);
        }

        // Check what should be in frustum
        println!("\nChecking mesh visibility:");
        for (mesh_gt, _mesh, name) in q_meshes.iter() {
            let dist = (cam_gt.translation() - mesh_gt.translation()).length();
            println!("  {} at distance {:.1} km", name.as_str(), dist / 1000.0);

            // Basic frustum check
            if let Projection::Perspective(persp) = projection {
                if dist >= persp.near && dist <= persp.far {
                    println!("    -> Within near/far planes!");
                } else {
                    println!("    -> OUTSIDE near/far planes!");
                }
            }
        }
    } else {
        println!("ERROR: No camera with FloatingOrigin found!");
    }
}

/// Emit detailed per-frame camera diagnostics when verbose logging is enabled.
fn debug_camera_position(
    q_cam: Query<(&GlobalTransform, &CellCoord, &Transform, &OrbitState), With<Camera>>,
    q_earth: Query<(&GlobalTransform, &CellCoord, &Transform, &Name), Without<Camera>>,
    q_meshes: Query<(Entity, &Name, &GlobalTransform, Option<&Mesh3d>)>,
    mut frame_count: Local<u32>,
) {
    *frame_count += 1;

    // Print every 60 frames
    if *frame_count % 60 != 0 {
        return;
    }

    if let Ok((cam_gt, cell, transform, orbit)) = q_cam.single() {
        println!("\n=== Frame {} Debug ===", *frame_count);
        println!(
            "Camera: cell=[{}, {}, {}] local={:.1},{:.1},{:.1}",
            cell.x,
            cell.y,
            cell.z,
            transform.translation.x,
            transform.translation.y,
            transform.translation.z
        );
        println!(
            "  GlobalTransform: pos={:.1},{:.1},{:.1}",
            cam_gt.translation().x,
            cam_gt.translation().y,
            cam_gt.translation().z
        );
        println!(
            "  Orbit: dist={:.1}km yaw={:.2}° pitch={:.2}°",
            orbit.distance / 1000.0,
            orbit.yaw.to_degrees(),
            orbit.pitch.to_degrees()
        );

        // Check all meshes
        println!("\nAll meshes in scene:");
        for (entity, name, gt, mesh) in q_meshes.iter() {
            let mesh_info = if mesh.is_some() {
                "HAS MESH"
            } else {
                "NO MESH"
            };
            println!(
                "  Entity {:?} '{}': {} at {:.1},{:.1},{:.1}",
                entity,
                name.as_str(),
                mesh_info,
                gt.translation().x,
                gt.translation().y,
                gt.translation().z
            );

            // Calculate distance from camera
            let dist = (cam_gt.translation() - gt.translation()).length();
            println!("    Distance from camera: {:.1} km", dist / 1000.0);

            // Check if it should be visible
            if dist < (orbit.distance * 2.0) as f32 {
                println!("    >>> SHOULD BE VISIBLE <<<");
            }
        }

        // Check Earth specifically
        for (earth_gt, earth_cell, earth_transform, name) in q_earth.iter() {
            if name.as_str() == "Earth" {
                println!("\nEarth Entity Details:");
                println!(
                    "  Cell: [{}, {}, {}]",
                    earth_cell.x, earth_cell.y, earth_cell.z
                );
                println!(
                    "  Local Transform: pos={:.1},{:.1},{:.1} scale={:.1},{:.1},{:.1}",
                    earth_transform.translation.x,
                    earth_transform.translation.y,
                    earth_transform.translation.z,
                    earth_transform.scale.x,
                    earth_transform.scale.y,
                    earth_transform.scale.z
                );
                println!(
                    "  GlobalTransform: pos={:.1},{:.1},{:.1}",
                    earth_gt.translation().x,
                    earth_gt.translation().y,
                    earth_gt.translation().z
                );

                let distance = (cam_gt.translation() - earth_gt.translation()).length();
                println!("  Distance to camera: {:.1} km", distance / 1000.0);
            }
        }
    }

    // First frame - detailed spawn check
    if *frame_count == 60 {
        println!("\n=== INITIAL SPAWN CHECK ===");
        let earth_count = q_earth
            .iter()
            .filter(|(_, _, _, n)| n.as_str() == "Earth")
            .count();
        let debug_count = q_earth
            .iter()
            .filter(|(_, _, _, n)| n.as_str() == "DebugSphere")
            .count();
        let cam_count = q_cam.iter().count();
        let total_meshes = q_meshes.iter().count();
        println!("Cameras: {}", cam_count);
        println!("Earth entities: {}", earth_count);
        println!("Debug sphere entities: {}", debug_count);
        println!("Total entities with names: {}", total_meshes);
    }
}
