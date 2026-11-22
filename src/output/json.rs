use crate::core::LineageDossier;

/// Render the archaeological dossier as structured JSON
pub fn render_json(dossier: &LineageDossier) -> Result<String, String> {
    serde_json::to_string_pretty(dossier).map_err(|e| format!("Failed to serialize JSON: {e}"))
}
