//! Persistent player-facing application settings.

use std::{fs, path::PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::world::SECTION_SIZE_M;

const SETTINGS_FILE: &str = "save/settings.json";
const SETTINGS_FORMAT_VERSION: u32 = 2;
const QUARTER_METRE_SECTION_SIZE_M: f32 = 8.0;

/// 8 sections = 256 m: enough horizon for mountain massifs to read as
/// silhouettes (Hytale-scale vistas) while staying streamable.
pub const DEFAULT_RENDER_DISTANCE_SECTIONS: i32 = 8;
pub const MIN_RENDER_DISTANCE_SECTIONS: i32 = 2;
pub const MAX_RENDER_DISTANCE_SECTIONS: i32 = 12;
pub const RENDER_DISTANCE_STEP: i32 = 1;
pub const UNLOAD_MARGIN_SECTIONS: i32 = 1;

#[derive(Debug, Clone, Copy, Resource, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct WorldSettings {
    #[serde(default)]
    format_version: u32,
    render_distance_sections: i32,
}

impl Default for WorldSettings {
    fn default() -> Self {
        Self {
            format_version: SETTINGS_FORMAT_VERSION,
            render_distance_sections: DEFAULT_RENDER_DISTANCE_SECTIONS,
        }
    }
}

impl WorldSettings {
    pub fn load_or_default() -> Self {
        let Some(settings) = fs::read_to_string(settings_file_path())
            .ok()
            .and_then(|json| serde_json::from_str::<Self>(&json).ok())
        else {
            return Self::default();
        };
        settings.migrated().validated()
    }

    pub fn render_distance_sections(&self) -> i32 {
        self.render_distance_sections
    }

    pub fn unload_distance_sections(&self) -> i32 {
        self.render_distance_sections + UNLOAD_MARGIN_SECTIONS
    }

    pub fn render_distance_m(&self) -> f32 {
        self.render_distance_sections as f32 * SECTION_SIZE_M
    }

    pub fn change_render_distance(&mut self, direction: i32) -> bool {
        let old = self.render_distance_sections;
        self.render_distance_sections = (old + direction.signum() * RENDER_DISTANCE_STEP)
            .clamp(MIN_RENDER_DISTANCE_SECTIONS, MAX_RENDER_DISTANCE_SECTIONS);
        self.render_distance_sections != old
    }

    pub fn write_to_disk(&self) {
        let path = settings_file_path();
        let result = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)
            .and_then(|json| {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, json)
            });
        if let Err(error) = result {
            error!("Failed to write settings: {error}");
        }
    }

    fn validated(mut self) -> Self {
        self.render_distance_sections = self
            .render_distance_sections
            .clamp(MIN_RENDER_DISTANCE_SECTIONS, MAX_RENDER_DISTANCE_SECTIONS);
        self.render_distance_sections -= self
            .render_distance_sections
            .rem_euclid(RENDER_DISTANCE_STEP);
        self
    }

    fn migrated(mut self) -> Self {
        if self.format_version < SETTINGS_FORMAT_VERSION {
            let distance_m = self.render_distance_sections as f32 * QUARTER_METRE_SECTION_SIZE_M;
            self.render_distance_sections = (distance_m / SECTION_SIZE_M).round() as i32;
            self.format_version = SETTINGS_FORMAT_VERSION;
        }
        self
    }
}

fn settings_file_path() -> PathBuf {
    std::env::var_os("RUNEWILD_SETTINGS_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SETTINGS_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_saved_render_distances_are_clamped_and_aligned() {
        let too_small: WorldSettings =
            serde_json::from_str(r#"{"format_version":2,"render_distance_sections":-7}"#).unwrap();
        let uneven: WorldSettings =
            serde_json::from_str(r#"{"format_version":2,"render_distance_sections":7}"#).unwrap();
        let too_large: WorldSettings =
            serde_json::from_str(r#"{"format_version":2,"render_distance_sections":99}"#).unwrap();

        assert_eq!(
            too_small.validated().render_distance_sections(),
            MIN_RENDER_DISTANCE_SECTIONS
        );
        assert_eq!(uneven.validated().render_distance_sections(), 7);
        assert_eq!(
            too_large.validated().render_distance_sections(),
            MAX_RENDER_DISTANCE_SECTIONS
        );
    }

    #[test]
    fn render_distance_stepper_respects_bounds_and_reports_changes() {
        let mut settings = WorldSettings::default();
        assert!(settings.change_render_distance(1));
        assert_eq!(settings.render_distance_sections(), 6);

        settings.render_distance_sections = MAX_RENDER_DISTANCE_SECTIONS;
        assert!(!settings.change_render_distance(1));
        assert!(settings.change_render_distance(-1));

        settings.render_distance_sections = MIN_RENDER_DISTANCE_SECTIONS;
        assert!(!settings.change_render_distance(-1));
    }

    #[test]
    fn displayed_distance_uses_physical_section_size() {
        let settings = WorldSettings::default();
        assert_eq!(settings.render_distance_m(), 160.0);
        assert_eq!(settings.unload_distance_sections(), 6);
    }

    #[test]
    fn quarter_metre_settings_preserve_approximately_the_same_distance() {
        let legacy: WorldSettings =
            serde_json::from_str(r#"{"render_distance_sections":10}"#).unwrap();
        let migrated = legacy.migrated().validated();
        assert_eq!(migrated.render_distance_sections(), 3);
        assert_eq!(migrated.render_distance_m(), 96.0);
    }
}
