use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
    window::PrimaryWindow,
};

use crate::{
    game::WORLD_SEED,
    player::{
        controller::{Controller, CursorState},
        Player,
    },
};

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub text: String,
    pub color: Color,
    pub timestamp: f32,
}

#[derive(Debug, Resource)]
pub struct ChatState {
    pub is_open: bool,
    pub input: String,
    pub messages: Vec<ChatMessage>,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            is_open: false,
            input: String::new(),
            messages: vec![ChatMessage {
                text:
                    "Welcome to Runewild! Press Enter or / to open chat, type /help for commands."
                        .to_string(),
                color: Color::srgb(0.95, 0.82, 0.45),
                timestamp: 0.0,
            }],
            history: Vec::new(),
            history_index: None,
        }
    }
}

impl ChatState {
    pub fn add_message(&mut self, text: impl Into<String>, color: Color, time: f32) {
        self.messages.push(ChatMessage {
            text: text.into(),
            color,
            timestamp: time,
        });
        if self.messages.len() > 60 {
            self.messages.remove(0);
        }
    }
}

#[derive(Component)]
pub struct ChatContainer;

#[derive(Component)]
pub struct ChatLogText;

#[derive(Component)]
pub struct ChatInputContainer;

#[derive(Component)]
pub struct ChatInputText;

pub fn setup_chat(mut commands: Commands) {
    commands
        .spawn((
            ChatContainer,
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    bottom: Val::Px(88.0),
                    left: Val::Px(16.0),
                    width: Val::Px(480.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    ..default()
                },
                ..default()
            },
        ))
        .with_children(|parent| {
            // Message log panel
            parent
                .spawn(NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        min_height: Val::Px(120.0),
                        max_height: Val::Px(180.0),
                        padding: UiRect::all(Val::Px(8.0)),
                        flex_direction: FlexDirection::Column,
                        justify_content: JustifyContent::FlexEnd,
                        ..default()
                    },
                    background_color: Color::srgba(0.06, 0.05, 0.04, 0.45).into(),
                    ..default()
                })
                .with_children(|log_parent| {
                    log_parent.spawn((
                        ChatLogText,
                        TextBundle::from_section(
                            "",
                            TextStyle {
                                font_size: 13.5,
                                color: Color::srgb(0.95, 0.95, 0.95),
                                ..default()
                            },
                        ),
                    ));
                });

            // Input line panel
            parent
                .spawn((
                    ChatInputContainer,
                    NodeBundle {
                        style: Style {
                            width: Val::Percent(100.0),
                            height: Val::Px(32.0),
                            padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                            border: UiRect::all(Val::Px(1.5)),
                            display: Display::None,
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        background_color: Color::srgba(0.08, 0.06, 0.04, 0.85).into(),
                        border_color: Color::srgba(0.95, 0.76, 0.42, 0.70).into(),
                        ..default()
                    },
                ))
                .with_children(|input_parent| {
                    input_parent.spawn((
                        ChatInputText,
                        TextBundle::from_section(
                            "> ",
                            TextStyle {
                                font_size: 14.0,
                                color: Color::srgb(1.0, 0.96, 0.82),
                                ..default()
                            },
                        ),
                    ));
                });
        });
}

#[allow(clippy::too_many_arguments)]
pub fn handle_chat_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut keyboard_events: EventReader<KeyboardInput>,
    mut chat_state: ResMut<ChatState>,
    mut cursor: ResMut<CursorState>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut player: Query<(&mut Transform, &mut Controller), With<Player>>,
) {
    let current_time = time.elapsed_seconds();

    // 1. Open chat when pressing Enter, / or T while closed
    if !chat_state.is_open {
        if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::KeyT) {
            chat_state.is_open = true;
            chat_state.input.clear();
            chat_state.history_index = None;
            cursor.set_grabbed(false);
            if let Ok(mut window) = windows.get_single_mut() {
                window.cursor.grab_mode = bevy::window::CursorGrabMode::None;
                window.cursor.visible = true;
            }
            return;
        }
        if keys.just_pressed(KeyCode::Slash) {
            chat_state.is_open = true;
            chat_state.input = "/".to_string();
            chat_state.history_index = None;
            cursor.set_grabbed(false);
            if let Ok(mut window) = windows.get_single_mut() {
                window.cursor.grab_mode = bevy::window::CursorGrabMode::None;
                window.cursor.visible = true;
            }
            return;
        }
        return;
    }

    // 2. Chat is open: process keys
    if keys.just_pressed(KeyCode::Escape) {
        chat_state.is_open = false;
        chat_state.input.clear();
        chat_state.history_index = None;
        cursor.set_grabbed(true);
        if let Ok(mut window) = windows.get_single_mut() {
            window.cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
            window.cursor.visible = false;
        }
        return;
    }

    if keys.just_pressed(KeyCode::Enter) || keys.just_pressed(KeyCode::NumpadEnter) {
        let input = chat_state.input.trim().to_string();
        if !input.is_empty() {
            chat_state.history.push(input.clone());
            if input.starts_with('/') {
                execute_command(&input, &mut chat_state, current_time, &mut player);
            } else {
                chat_state.add_message(
                    format!("<Player> {}", input),
                    Color::srgb(0.92, 0.92, 0.92),
                    current_time,
                );
            }
        }
        chat_state.input.clear();
        chat_state.history_index = None;
        chat_state.is_open = false;
        cursor.set_grabbed(true);
        if let Ok(mut window) = windows.get_single_mut() {
            window.cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
            window.cursor.visible = false;
        }
        return;
    }

    // Command history navigation
    if keys.just_pressed(KeyCode::ArrowUp) && !chat_state.history.is_empty() {
        let new_index = match chat_state.history_index {
            None => chat_state.history.len().saturating_sub(1),
            Some(i) => i.saturating_sub(1),
        };
        chat_state.history_index = Some(new_index);
        if let Some(cmd) = chat_state.history.get(new_index) {
            chat_state.input = cmd.clone();
        }
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        if let Some(i) = chat_state.history_index {
            if i + 1 < chat_state.history.len() {
                let new_index = i + 1;
                chat_state.history_index = Some(new_index);
                if let Some(cmd) = chat_state.history.get(new_index) {
                    chat_state.input = cmd.clone();
                }
            } else {
                chat_state.history_index = None;
                chat_state.input.clear();
            }
        }
    }

    // Character input
    for event in keyboard_events.read() {
        if !event.state.is_pressed() {
            continue;
        }
        match &event.logical_key {
            Key::Character(s) => {
                chat_state.input.push_str(s);
            }
            Key::Space => {
                chat_state.input.push(' ');
            }
            _ => {
                if event.key_code == KeyCode::Backspace {
                    chat_state.input.pop();
                }
            }
        }
    }
}

pub fn parse_and_apply_command(
    command_line: &str,
    chat_state: &mut ChatState,
    current_time: f32,
    player_target: Option<(&mut Transform, &mut Controller)>,
) {
    let parts: Vec<&str> = command_line.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    let cmd = parts[0].to_lowercase();
    match cmd.as_str() {
        "/set" => {
            if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("speed") {
                if let Ok(speed) = parts[2].parse::<f32>() {
                    let clamped = speed.clamp(0.1, 20.0);
                    if let Some((_, controller)) = player_target {
                        controller.speed_multiplier = clamped;
                    }
                    chat_state.add_message(
                        format!("[Speed] Movement speed multiplier set to {:.2}x", clamped),
                        Color::srgb(0.42, 0.92, 0.46),
                        current_time,
                    );
                } else {
                    chat_state.add_message(
                        "Usage: /set speed <number> (e.g. /set speed 2.5)",
                        Color::srgb(0.95, 0.42, 0.38),
                        current_time,
                    );
                }
            } else {
                chat_state.add_message(
                    "Usage: /set speed <number>",
                    Color::srgb(0.95, 0.42, 0.38),
                    current_time,
                );
            }
        }
        "/speed" => {
            if parts.len() >= 2 {
                if let Ok(speed) = parts[1].parse::<f32>() {
                    let clamped = speed.clamp(0.1, 20.0);
                    if let Some((_, controller)) = player_target {
                        controller.speed_multiplier = clamped;
                    }
                    chat_state.add_message(
                        format!("[Speed] Movement speed multiplier set to {:.2}x", clamped),
                        Color::srgb(0.42, 0.92, 0.46),
                        current_time,
                    );
                } else {
                    chat_state.add_message(
                        "Usage: /speed <number> (e.g. /speed 2.0)",
                        Color::srgb(0.95, 0.42, 0.38),
                        current_time,
                    );
                }
            } else {
                chat_state.add_message(
                    "Usage: /speed <number>",
                    Color::srgb(0.95, 0.42, 0.38),
                    current_time,
                );
            }
        }
        "/tp" | "/teleport" => {
            if parts.len() == 4 {
                if let (Ok(x), Ok(y), Ok(z)) = (
                    parts[1].parse::<f32>(),
                    parts[2].parse::<f32>(),
                    parts[3].parse::<f32>(),
                ) {
                    if let Some((transform, controller)) = player_target {
                        transform.translation = Vec3::new(x, y, z);
                        controller.velocity = Vec3::ZERO;
                    }
                    chat_state.add_message(
                        format!("[Teleport] Teleported to ({:.1}, {:.1}, {:.1})", x, y, z),
                        Color::srgb(0.42, 0.88, 0.95),
                        current_time,
                    );
                } else {
                    chat_state.add_message(
                        "Usage: /tp <x> <y> <z> (e.g. /tp 0 25 0)",
                        Color::srgb(0.95, 0.42, 0.38),
                        current_time,
                    );
                }
            } else {
                chat_state.add_message(
                    "Usage: /tp <x> <y> <z>",
                    Color::srgb(0.95, 0.42, 0.38),
                    current_time,
                );
            }
        }
        "/fly" | "/flight" => {
            let mut status = "TOGGLED";
            if let Some((_, controller)) = player_target {
                controller.flying = !controller.flying;
                status = if controller.flying { "ON" } else { "OFF" };
            }
            chat_state.add_message(
                format!("[Flight] Flight mode: {}", status),
                Color::srgb(0.95, 0.82, 0.35),
                current_time,
            );
        }
        "/seed" => {
            chat_state.add_message(
                format!("[World] Seed: {} ({:#x})", WORLD_SEED, WORLD_SEED),
                Color::srgb(0.85, 0.76, 0.95),
                current_time,
            );
        }
        "/clear" => {
            chat_state.messages.clear();
        }
        "/help" => {
            chat_state.add_message(
                "--- Available Commands ---",
                Color::srgb(0.95, 0.82, 0.45),
                current_time,
            );
            chat_state.add_message(
                "/set speed <val> - Set player movement speed (0.1 - 20.0)",
                Color::srgb(0.88, 0.88, 0.88),
                current_time,
            );
            chat_state.add_message(
                "/tp <x> <y> <z>   - Teleport to coordinates",
                Color::srgb(0.88, 0.88, 0.88),
                current_time,
            );
            chat_state.add_message(
                "/fly             - Toggle flying mode",
                Color::srgb(0.88, 0.88, 0.88),
                current_time,
            );
            chat_state.add_message(
                "/seed            - Show world generator seed",
                Color::srgb(0.88, 0.88, 0.88),
                current_time,
            );
            chat_state.add_message(
                "/clear           - Clear chat history",
                Color::srgb(0.88, 0.88, 0.88),
                current_time,
            );
        }
        _ => {
            chat_state.add_message(
                format!(
                    "Unknown command: '{}'. Type /help for a list of commands.",
                    cmd
                ),
                Color::srgb(0.95, 0.42, 0.38),
                current_time,
            );
        }
    }
}

fn execute_command(
    command_line: &str,
    chat_state: &mut ChatState,
    current_time: f32,
    player: &mut Query<(&mut Transform, &mut Controller), With<Player>>,
) {
    let mut player_opt = match player.get_single_mut() {
        Ok((t, c)) => Some((t, c)),
        Err(_) => None,
    };
    let target = player_opt.as_mut().map(|(t, c)| (&mut **t, &mut **c));
    parse_and_apply_command(command_line, chat_state, current_time, target);
}

pub fn update_chat_ui(
    time: Res<Time>,
    chat_state: Res<ChatState>,
    mut log_query: Query<&mut Text, (With<ChatLogText>, Without<ChatInputText>)>,
    mut input_container_query: Query<&mut Style, With<ChatInputContainer>>,
    mut input_text_query: Query<&mut Text, (With<ChatInputText>, Without<ChatLogText>)>,
) {
    let current_time = time.elapsed_seconds();

    // 1. Update message log text
    if let Ok(mut log_text) = log_query.get_single_mut() {
        let max_display = if chat_state.is_open { 8 } else { 5 };
        let mut display_lines = Vec::new();

        for msg in chat_state.messages.iter().rev() {
            let age = current_time - msg.timestamp;
            if chat_state.is_open || age < 8.0 {
                display_lines.push(msg.clone());
                if display_lines.len() >= max_display {
                    break;
                }
            }
        }
        display_lines.reverse();

        log_text.sections.clear();
        for (i, msg) in display_lines.iter().enumerate() {
            if i > 0 {
                log_text.sections.push(TextSection {
                    value: "\n".to_string(),
                    style: TextStyle {
                        font_size: 13.5,
                        color: Color::NONE,
                        ..default()
                    },
                });
            }
            let alpha = if chat_state.is_open {
                1.0
            } else {
                let age = current_time - msg.timestamp;
                (1.0 - (age - 6.0) / 2.0).clamp(0.0, 1.0)
            };
            let mut col = msg.color;
            if let Color::Srgba(ref mut srgba) = col {
                srgba.alpha *= alpha;
            }
            log_text.sections.push(TextSection {
                value: msg.text.clone(),
                style: TextStyle {
                    font_size: 13.5,
                    color: col,
                    ..default()
                },
            });
        }
    }

    // 2. Toggle input container visibility
    if let Ok(mut input_style) = input_container_query.get_single_mut() {
        input_style.display = if chat_state.is_open {
            Display::Flex
        } else {
            Display::None
        };
    }

    // 3. Update input line text with blinking cursor
    if let Ok(mut input_text) = input_text_query.get_single_mut() {
        if chat_state.is_open {
            let blink = (current_time * 2.5).fract() < 0.5;
            let cursor = if blink { "_" } else { " " };
            input_text.sections[0].value = format!("> {}{}", chat_state.input, cursor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_state_appends_messages_and_limits_capacity() {
        let mut chat = ChatState::default();
        for i in 0..100 {
            chat.add_message(format!("Msg {}", i), Color::WHITE, i as f32);
        }
        assert!(chat.messages.len() <= 60);
    }

    #[test]
    fn speed_command_adjusts_controller_multiplier() {
        let mut chat = ChatState::default();
        let mut transform = Transform::IDENTITY;
        let mut controller = Controller {
            velocity: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            grounded: true,
            sensitivity: 0.002,
            flying: false,
            space_pressed: false,
            last_space_press_time: 0.0,
            speed_multiplier: 1.0,
        };

        parse_and_apply_command(
            "/set speed 3.5",
            &mut chat,
            1.0,
            Some((&mut transform, &mut controller)),
        );
        assert!((controller.speed_multiplier - 3.5).abs() < 0.001);
        assert!(chat.messages.last().unwrap().text.contains("3.50x"));

        parse_and_apply_command(
            "/speed 5.0",
            &mut chat,
            2.0,
            Some((&mut transform, &mut controller)),
        );
        assert!((controller.speed_multiplier - 5.0).abs() < 0.001);
    }

    #[test]
    fn teleport_and_flight_commands_work() {
        let mut chat = ChatState::default();
        let mut transform = Transform::IDENTITY;
        let mut controller = Controller {
            velocity: Vec3::ONE,
            yaw: 0.0,
            pitch: 0.0,
            grounded: true,
            sensitivity: 0.002,
            flying: false,
            space_pressed: false,
            last_space_press_time: 0.0,
            speed_multiplier: 1.0,
        };

        parse_and_apply_command(
            "/tp 10 30 -50",
            &mut chat,
            1.0,
            Some((&mut transform, &mut controller)),
        );
        assert_eq!(transform.translation, Vec3::new(10.0, 30.0, -50.0));
        assert_eq!(controller.velocity, Vec3::ZERO);

        parse_and_apply_command(
            "/fly",
            &mut chat,
            2.0,
            Some((&mut transform, &mut controller)),
        );
        assert!(controller.flying);
    }
}
