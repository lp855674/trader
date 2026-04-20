#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error(transparent)]
    Db(#[from] db::DbError),
    #[error("execution_not_configured: live adapter is a stub")]
    NotConfigured,
    #[error("invalid_order_request: {0}")]
    InvalidOrderRequest(String),
    #[error("longbridge: {0}")]
    Longbridge(String),
}

impl ExecError {
    pub const ERROR_CODE_NOT_CONFIGURED: &'static str = "execution_not_configured";
    pub const ERROR_CODE_INVALID_ORDER_REQUEST: &'static str = "invalid_order_request";
}
