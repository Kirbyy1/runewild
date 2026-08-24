use bevy::{pbr::CascadeShadowConfigBuilder, prelude::*};

pub fn setup_atmosphere(mut commands: Commands) {
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.72, 0.82, 1.0),
        brightness: 285.0,
    });

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            color: Color::srgb(1.0, 0.91, 0.76),
            illuminance: 24_000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.92, -0.68, -0.18)),
        cascade_shadow_config: CascadeShadowConfigBuilder {
            first_cascade_far_bound: 20.0,
            maximum_distance: 150.0,
            ..default()
        }
        .into(),
        ..default()
    });
}
