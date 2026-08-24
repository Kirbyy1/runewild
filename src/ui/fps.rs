use bevy::{
    diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin},
    prelude::*,
};

#[derive(Component)]
pub struct FpsText;

pub fn setup_fps_counter(mut commands: Commands) {
    commands
        .spawn(NodeBundle {
            style: Style {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                border: UiRect::all(Val::Px(1.0)),
                display: Display::Flex,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            background_color: Color::srgba(0.08, 0.06, 0.04, 0.65).into(),
            border_color: Color::srgba(0.95, 0.76, 0.42, 0.40).into(),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                FpsText,
                TextBundle::from_section(
                    "FPS: --",
                    TextStyle {
                        font_size: 14.0,
                        color: Color::srgb(0.98, 0.92, 0.78),
                        ..default()
                    },
                ),
            ));
        });
}

pub fn update_fps_counter(
    diagnostics: Res<DiagnosticsStore>,
    mut query: Query<&mut Text, With<FpsText>>,
) {
    let Ok(mut text) = query.get_single_mut() else {
        return;
    };

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .unwrap_or(0.0);

    let frame_time = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|ft| ft.smoothed())
        .unwrap_or(0.0);

    let color = if fps >= 55.0 {
        Color::srgb(0.42, 0.92, 0.46)
    } else if fps >= 30.0 {
        Color::srgb(0.95, 0.82, 0.35)
    } else {
        Color::srgb(0.95, 0.38, 0.32)
    };

    text.sections[0].value = format!("FPS: {:.0} ({:.1} ms)", fps, frame_time);
    text.sections[0].style.color = color;
}
