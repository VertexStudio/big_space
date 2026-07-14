//! Author a `big_space` world declaratively with Bevy 0.19's BSN scenes (`bsn!`).
//!
//! All `big_space` components are `Default + Clone`, which gives them BSN templates for free.
//! Combined with required components, a whole floating-origin world can be declared as data:
//! spawning [`BigSpace`] brings in `Grid` and `GlobalTransform`, [`FloatingOrigin`] brings in
//! [`CellCoord`], and [`CellCoord`] brings in `Transform` and `Visibility`.

use bevy::prelude::*;
use big_space::prelude::*;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<TransformPlugin>(),
            BigSpaceDefaultPlugins,
            CellHashingPlugin::default(),
            PartitionPlugin::default(),
        ))
        // Label occupied cells with their coordinates using bevy 0.19 text gizmos.
        .insert_resource(BigSpaceDebugSettings { label_cells: true })
        .add_systems(Startup, scene.spawn())
        .run();
}

/// A hand-authored constellation of occupied cells, declared entirely as a BSN scene.
fn scene() -> impl SceneList {
    bsn_list![(
        BigSpace
        Grid::new(10.0, 1.0)
        Children [
            (FloatingOrigin Camera3d Transform::from_xyz(0.0, 0.0, 60.0)),
            cell(3, 0, 0),
            cell(-3, 0, 0),
            cell(0, 3, 0),
            cell(0, -3, 0),
            cell(0, 0, 3),
            cell(0, 0, -3),
            cell(2, 2, 2),
            cell(-2, -2, -2),
        ]
    )]
}

/// A high-precision spatial entity at the given cell coordinates.
fn cell(x: GridPrecision, y: GridPrecision, z: GridPrecision) -> impl Scene {
    bsn! { CellCoord::new(x, y, z) }
}
