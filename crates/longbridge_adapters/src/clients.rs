use std::sync::Arc;

use longbridge::Config;
use longbridge::quote::QuoteContext;
use longbridge::trade::TradeContext;

/// Shared Longbridge SDK clients (quote + trade).  
/// 创建后会后台 drain push channel，避免内部缓冲无限增长。
pub struct LongbridgeClients {
    pub quote: Arc<QuoteContext>,
    pub trade: Arc<TradeContext>,
}

impl LongbridgeClients {
    /// Build clients from explicit credentials (e.g. read from DB).
    /// Use this instead of [`connect`] when credentials come from database config_json.
    pub fn connect_with_credentials(
        app_key: &str,
        app_secret: &str,
        access_token: &str,
    ) -> Result<Self, String> {
        let config = Arc::new(Config::from_apikey(app_key, app_secret, access_token));
        let (quote, mut q_rx) = QuoteContext::new(config.clone());
        let (trade, mut t_rx) = TradeContext::new(config);
        tokio::spawn(async move { while q_rx.recv().await.is_some() {} });
        tokio::spawn(async move { while t_rx.recv().await.is_some() {} });
        Ok(Self {
            quote: Arc::new(quote),
            trade: Arc::new(trade),
        })
    }

    /// 使用 `Config::from_apikey_env()`（需已设置 `LONGBRIDGE_*` 环境变量）。
    pub fn connect() -> Result<Self, String> {
        let config = Arc::new(Config::from_apikey_env().map_err(|e| e.to_string())?);
        let (quote, mut q_rx) = QuoteContext::new(config.clone());
        let (trade, mut t_rx) = TradeContext::new(config);
        tokio::spawn(async move { while q_rx.recv().await.is_some() {} });
        tokio::spawn(async move { while t_rx.recv().await.is_some() {} });
        Ok(Self {
            quote: Arc::new(quote),
            trade: Arc::new(trade),
        })
    }
}
