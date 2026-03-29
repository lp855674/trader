#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error(transparent)]
    Db(#[from] db::DbError),
    #[error("execution_not_configured: live adapter is a stub")]
    NotConfigured,
}

impl ExecError {
    pub const ERROR_CODE_NOT_CONFIGURED: &'static str = "execution_not_configured";
}
