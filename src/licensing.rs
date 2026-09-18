use crate::models::{AssetRow, LicenseAssessment, LicensePreset, LicenseStatus};

pub const LICENSE_PRESETS: &[LicensePreset] = &[
    LicensePreset {
        id: "CC0-1.0",
        name: "Creative Commons Zero v1.0 Universal",
        attribution_required: false,
        license_url: "https://creativecommons.org/publicdomain/zero/1.0/",
    },
    LicensePreset {
        id: "CC-BY-4.0",
        name: "Creative Commons Attribution 4.0 International",
        attribution_required: true,
        license_url: "https://creativecommons.org/licenses/by/4.0/",
    },
    LicensePreset {
        id: "Custom",
        name: "Custom / site-specific license",
        attribution_required: false,
        license_url: "",
    },
    LicensePreset {
        id: "Unknown",
        name: "Unknown / not yet verified",
        attribution_required: false,
        license_url: "",
    },
];

pub fn presets() -> &'static [LicensePreset] {
    LICENSE_PRESETS
}

pub fn normalize_license(value: Option<&str>) -> String {
    let raw = value.unwrap_or_default().trim();
    if raw.eq_ignore_ascii_case("cc0")
        || raw.eq_ignore_ascii_case("cc0 1.0")
        || raw.eq_ignore_ascii_case("cc0-1.0")
    {
        "CC0-1.0".to_string()
    } else if raw.eq_ignore_ascii_case("cc by 4.0")
        || raw.eq_ignore_ascii_case("cc-by-4.0")
        || raw.eq_ignore_ascii_case("creative commons attribution 4.0")
    {
        "CC-BY-4.0".to_string()
    } else if raw.eq_ignore_ascii_case("custom") {
        "Custom".to_string()
    } else if raw.is_empty() || raw.eq_ignore_ascii_case("unknown") {
        "Unknown".to_string()
    } else {
        raw.to_string()
    }
}

pub fn assess(row: &AssetRow) -> LicenseAssessment {
    let license_id = normalize_license(row.license.as_deref());
    let mut warnings = Vec::new();

    let status = match license_id.as_str() {
        "Unknown" => {
            warnings.push("License has not been verified.".to_string());
            LicenseStatus::Unknown
        }
        "Custom" => {
            if row.source_url.as_deref().is_none_or(str::is_empty) {
                warnings.push("Custom license should include a source URL or license reference.".to_string());
            }
            LicenseStatus::Custom
        }
        "CC0-1.0" => {
            if row.source_url.as_deref().is_none_or(str::is_empty) {
                warnings.push("Source URL is missing; provenance cannot be verified later.".to_string());
            }
            if warnings.is_empty() { LicenseStatus::Complete } else { LicenseStatus::Warning }
        }
        "CC-BY-4.0" => {
            if row.creator.as_deref().is_none_or(str::is_empty) {
                warnings.push("CC-BY-4.0 attribution is missing a creator/credit name.".to_string());
            }
            if row.source_url.as_deref().is_none_or(str::is_empty) {
                warnings.push("CC-BY-4.0 attribution is missing the source URL.".to_string());
            }
            if !row.attribution_required {
                warnings.push("CC-BY-4.0 requires attribution; attribution_required is currently disabled.".to_string());
            }
            if warnings.is_empty() { LicenseStatus::Complete } else { LicenseStatus::Warning }
        }
        _ => {
            warnings.push("License is not a recognized DragonForge preset; review its terms manually.".to_string());
            LicenseStatus::Custom
        }
    };

    LicenseAssessment {
        status,
        license_id,
        warnings,
    }
}

pub fn attribution_line(row: &AssetRow) -> String {
    let assessment = assess(row);
    let creator = row.creator.as_deref().unwrap_or("Unknown creator");
    let source = row.source_url.as_deref().unwrap_or("Source not recorded");
    match assessment.license_id.as_str() {
        "CC-BY-4.0" => format!(
            "{} — {} — {} — CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/)",
            row.name, creator, source
        ),
        "CC0-1.0" => format!(
            "{} — {} — {} — CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/)",
            row.name, creator, source
        ),
        license => format!("{} — {} — {} — {}", row.name, creator, source, license),
    }
}
