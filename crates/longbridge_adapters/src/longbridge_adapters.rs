//! Longbridge OpenAPI adapters (行情 K 线 + 实盘下单).  
//! 认证见官方文档：[快速开始](https://open.longbridge.com/zh-CN/docs/getting-started)（`LONGBRIDGE_APP_KEY` / `LONGBRIDGE_APP_SECRET` / `LONGBRIDGE_ACCESS_TOKEN`）。

mod clients;
mod exec_lb;
mod ingest_lb;

pub use clients::LongbridgeClients;
pub use exec_lb::LongbridgeTradeAdapter;
pub use ingest_lb::LongbridgeCandleIngest;
