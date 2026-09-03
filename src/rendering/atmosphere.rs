use bevy::{
    pbr::{CascadeShadowConfigBuilder, NotShadowCaster},
    prelude::*,
    render::{
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        texture::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    },
};
use noise::{NoiseFn, OpenSimplex};

use crate::player::Player;

/// World height of the drifting cloud sheet. Low enough that the sheet
/// stays inside the camera far plane from typical eye heights.
const CLOUD_LAYER_HEIGHT: f32 = 140.0;
/// Ground footprint of one seamless cloud texture tile.
const CLOUD_TILE_SIZE_M: f32 = 256.0;
/// Plane side length: covers the far-plane circle from any eye height the
/// game supports while remaining small enough to skip full-sky overdraw.
const CLOUD_PLANE_SIZE_M: f32 = 900.0;
const CLOUD_WIND_M_S: Vec2 = Vec2::new(1.15, -0.5);

#[derive(Component)]
pub(crate) struct CloudLayer;

pub fn setup_atmosphere(mut commands: Commands) {
    // Hytale-style daylight: a soft cool sky fill under a warm golden sun so
    // shaded foliage keeps colour instead of collapsing to grey.
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.72, 0.82, 1.0),
        brightness: 620.0,
    });

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::srgb(1.0, 0.93, 0.80),
            illuminance: 38_000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.92, -0.68, -0.18)),
        cascade_shadow_config: CascadeShadowConfigBuilder {
            first_cascade_far_bound: 64.0,
            maximum_distance: 160.0,
            ..default()
        }
        .into(),
        ..default()
    });
}

/// Spawns a soft unlit cloud sheet high above the world. It tracks the
/// player horizontally (snapped to the seamless tile period) and drifts
/// downwind, so the sky never runs out of clouds and the sheet never
/// intersects the camera far plane.
pub fn setup_clouds(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let texture = images.add(cloud_texture());
    let material = materials.add(StandardMaterial {
        base_color_texture: Some(texture),
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.78),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        cull_mode: None,
        ..default()
    });
    let mesh = meshes.add(cloud_plane_mesh());
    commands.spawn((
        CloudLayer,
        PbrBundle {
            mesh,
            material,
            transform: Transform::from_xyz(0.0, CLOUD_LAYER_HEIGHT, 0.0),
            ..default()
        },
        NotShadowCaster,
    ));
}

/// Keeps the sheet overhead. The pattern is periodic in `CLOUD_TILE_SIZE_M`,
/// so the plane may snap by whole tiles to stay near the player without any
/// visible pop.
pub fn drift_clouds(
    time: Res<Time>,
    player: Query<&Transform, With<Player>>,
    mut clouds: Query<&mut Transform, (With<CloudLayer>, Without<Player>)>,
) {
    let Ok(player) = player.get_single() else {
        return;
    };
    let Ok(mut cloud) = clouds.get_single_mut() else {
        return;
    };
    let elapsed = time.elapsed_seconds();
    let drift_x = elapsed * CLOUD_WIND_M_S.x;
    let drift_z = elapsed * CLOUD_WIND_M_S.y;

    let tile = CLOUD_TILE_SIZE_M;
    let wrap = |value: f32| {
        let wrapped = value.rem_euclid(tile);
        if wrapped > tile * 0.5 {
            wrapped - tile
        } else {
            wrapped
        }
    };
    let offset_x = wrap(drift_x - player.translation.x);
    let offset_z = wrap(drift_z - player.translation.z);

    cloud.translation = Vec3::new(
        player.translation.x + offset_x,
        CLOUD_LAYER_HEIGHT,
        player.translation.z + offset_z,
    );
}

/// Horizontal quad fan (center + 4 corners) with UVs tiling the seamless
/// cloud texture and a radial alpha falloff so the plane boundary never
/// shows as a hard band against the sky.
fn cloud_plane_mesh() -> Mesh {
    let half = CLOUD_PLANE_SIZE_M * 0.5;
    let tiles = CLOUD_PLANE_SIZE_M / CLOUD_TILE_SIZE_M;
    let mut mesh = Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    // 0 = center, 1..4 = corners (+x+z, -x+z, -x-z, +x-z).
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [0.0, 0.0, 0.0],
            [half, 0.0, half],
            [-half, 0.0, half],
            [-half, 0.0, -half],
            [half, 0.0, -half],
        ],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; 5]);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![
            [tiles * 0.5, tiles * 0.5],
            [tiles, 0.0],
            [0.0, 0.0],
            [0.0, tiles],
            [tiles, tiles],
        ],
    );
    // Center opaque, corners fully faded.
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_COLOR,
        vec![
            [1.0, 1.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0, 0.0],
        ],
    );
    mesh.insert_indices(bevy::render::mesh::Indices::U32(vec![
        0, 2, 1, 0, 3, 2, 0, 4, 3, 0, 1, 4,
    ]));
    mesh
}

/// Seamlessly tiling fBm alpha mask: 4D OpenSimplex sampled on a torus so
/// opposite edges match exactly and the sheet can wrap without seams.
fn cloud_texture() -> Image {
    const RES: u32 = 256;
    const TORUS_RADIUS: f64 = 2.4;
    let noise = OpenSimplex::new(0xC10D_5EED);
    let mut pixels = Vec::with_capacity((RES * RES * 4) as usize);
    for y in 0..RES {
        let v = f64::from(y) / f64::from(RES);
        let angle_v = v * std::f64::consts::TAU;
        let (cv, sv) = (angle_v.cos() * TORUS_RADIUS, angle_v.sin() * TORUS_RADIUS);
        for x in 0..RES {
            let u = f64::from(x) / f64::from(RES);
            let angle_u = u * std::f64::consts::TAU;
            let (cu, su) = (angle_u.cos() * TORUS_RADIUS, angle_u.sin() * TORUS_RADIUS);

            let mut amplitude = 1.0f64;
            let mut frequency = 1.0f64;
            let mut sum = 0.0f64;
            let mut norm = 0.0f64;
            for _ in 0..4 {
                sum += noise.get([
                    cu * frequency,
                    su * frequency,
                    cv * frequency,
                    sv * frequency,
                ]) * amplitude;
                norm += amplitude;
                amplitude *= 0.55;
                frequency *= 2.1;
            }
            let coverage = ((sum / norm) * 0.5 + 0.5).clamp(0.0, 1.0);
            let t = ((coverage - 0.50) / (0.70 - 0.50)).clamp(0.0, 1.0);
            let shaped = t * t * (3.0 - 2.0 * t);
            pixels.extend_from_slice(&[255, 255, 255, (shaped * 255.0) as u8]);
        }
    }

    let mut image = Image::new(
        Extent3d {
            width: RES,
            height: RES,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}
