#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SkillRunMode {
    Chat,
    Test,
    Cron,
}
