use thiserror::Error;

#[derive(Debug, Error)]
pub enum Kr0kiError {
    #[error("chart build failed: {0}")]
    ChartBuild(String),
    #[error("no ActionDefinition root found in element set")]
    NoRoot,
    #[error("duplicate element id: {0}")]
    DuplicateId(String),
}
