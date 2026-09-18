use crate::{
    error::{AppError, AppResult},
    models::{EnginePreset, Project, ProjectExportPlan},
};
use std::path::{Path, PathBuf};

pub const ENGINE_PRESETS: &[EnginePreset] = &[
    EnginePreset {
        id: "Generic",
        display_name: "Generic",
        export_subdir: "DragonForgeAssets",
        project_markers: &[],
        import_notes: "Assets remain in DragonForgeAssets under the project root.",
    },
    EnginePreset {
        id: "Godot",
        display_name: "Godot",
        export_subdir: "assets/dragonforge",
        project_markers: &["project.godot"],
        import_notes: "Godot imports supported resources automatically after they appear under the project root. DragonForge exports into assets/dragonforge.",
    },
    EnginePreset {
        id: "Unity",
        display_name: "Unity",
        export_subdir: "Assets/DragonForge",
        project_markers: &["Assets", "ProjectSettings"],
        import_notes: "Unity's AssetDatabase monitors Assets/. DragonForge exports into Assets/DragonForge so Unity can import supported files automatically.",
    },
    EnginePreset {
        id: "Unreal",
        display_name: "Unreal Engine",
        export_subdir: "Content/DragonForge",
        project_markers: &["Content"],
        import_notes: "DragonForge stages source assets under Content/DragonForge. Some source formats still require import through Unreal Editor; DragonForge does not create .uasset files.",
    },
    EnginePreset {
        id: "Roblox",
        display_name: "Roblox / Rojo",
        export_subdir: "src/DragonForgeAssets",
        project_markers: &["default.project.json"],
        import_notes: "For Rojo-style projects DragonForge exports into src/DragonForgeAssets. Roblox Studio-native assets may still require conversion or import through Studio.",
    },
    EnginePreset {
        id: "Minecraft",
        display_name: "Minecraft Bedrock",
        export_subdir: "DragonForgeAssets",
        project_markers: &["manifest.json"],
        import_notes: "DragonForge preserves source assets under DragonForgeAssets. Minecraft Bedrock pack-specific placement and JSON references remain explicit because resource/behavior pack layouts vary by asset type.",
    },
];

pub fn presets() -> &'static [EnginePreset] {
    ENGINE_PRESETS
}

pub fn normalize(value: &str) -> Option<&'static EnginePreset> {
    let v = value.trim();
    ENGINE_PRESETS.iter().find(|preset| {
        preset.id.eq_ignore_ascii_case(v)
            || preset.display_name.eq_ignore_ascii_case(v)
            || (preset.id == "Unreal" && v.eq_ignore_ascii_case("Unreal Engine"))
            || (preset.id == "Roblox" && v.eq_ignore_ascii_case("Rojo"))
            || (preset.id == "Minecraft" && v.eq_ignore_ascii_case("Minecraft Bedrock"))
    })
}

pub fn require(value: &str) -> AppResult<&'static EnginePreset> {
    normalize(value).ok_or_else(|| {
        AppError::BadRequest(format!(
            "unsupported project engine '{value}'; use Generic, Godot, Unity, Unreal, Roblox, or Minecraft"
        ))
    })
}

pub fn export_root(project: &Project) -> AppResult<PathBuf> {
    let preset = require(&project.engine)?;
    Ok(Path::new(&project.local_path).join(Path::new(preset.export_subdir)))
}

pub fn plan(project: &Project) -> AppResult<ProjectExportPlan> {
    let preset = require(&project.engine)?;
    let path = export_root(project)?;
    Ok(ProjectExportPlan {
        project_id: project.id.clone(),
        engine: preset.id.to_string(),
        export_subdir: preset.export_subdir.to_string(),
        export_path: path.display().to_string(),
        import_notes: preset.import_notes.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_supported_aliases() {
        assert_eq!(normalize("unreal engine").map(|p| p.id), Some("Unreal"));
        assert_eq!(normalize("rojo").map(|p| p.id), Some("Roblox"));
        assert_eq!(normalize("godot").map(|p| p.id), Some("Godot"));
    }

    #[test]
    fn rejects_unknown_engine() {
        assert!(require("UnknownEngine").is_err());
    }
}
