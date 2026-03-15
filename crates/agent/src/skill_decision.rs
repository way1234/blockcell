pub(crate) struct SkillDecisionEngine;

impl SkillDecisionEngine {
    pub(crate) fn normalize_selected_skill_name(
        selected: &str,
        candidates: &[(String, String)],
    ) -> Option<String> {
        let selected = selected.trim();
        if selected.is_empty() {
            return None;
        }

        if let Some((name, _)) = candidates.iter().find(|(name, _)| name == selected) {
            return Some(name.clone());
        }

        candidates
            .iter()
            .find(|(name, _)| selected.contains(name.as_str()) || name.contains(selected))
            .map(|(name, _)| name.clone())
    }
}
