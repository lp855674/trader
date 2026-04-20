use clap::{Args, Parser, Subcommand};
use terminal_core::models::{AmendOrderRequest, CancelOrderRequest, SubmitOrderRequest};

#[derive(Parser, Debug)]
#[command(name = "trader")]
pub struct Cli {
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    pub base_url: String,
    #[arg(long)]
    pub api_key: Option<String>,
    #[arg(long)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Tui,
    Quote {
        symbol: String,
    },
    Orders {
        #[command(subcommand)]
        action: OrdersCommand,
    },
    Order {
        #[command(subcommand)]
        action: OrderCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum OrdersCommand {
    List {
        #[arg(long)]
        account_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum OrderCommand {
    Submit(SubmitArgs),
    Cancel(CancelArgs),
    Amend(AmendArgs),
}

#[derive(Args, Debug)]
pub struct SubmitArgs {
    #[arg(long)]
    pub account_id: String,
    #[arg(long)]
    pub symbol: String,
    #[arg(long)]
    pub side: String,
    #[arg(long)]
    pub qty: f64,
    #[arg(long, default_value = "limit")]
    pub order_type: String,
    #[arg(long)]
    pub limit_price: f64,
}

impl SubmitArgs {
    pub fn into_request(self) -> SubmitOrderRequest {
        SubmitOrderRequest {
            account_id: self.account_id,
            symbol: self.symbol,
            side: self.side,
            qty: self.qty,
            order_type: self.order_type,
            limit_price: Some(self.limit_price),
        }
    }
}

#[derive(Args, Debug)]
pub struct CancelArgs {
    #[arg(long)]
    pub account_id: String,
    #[arg(long)]
    pub order_id: String,
}

impl CancelArgs {
    pub fn request(&self) -> CancelOrderRequest {
        CancelOrderRequest {
            account_id: self.account_id.clone(),
        }
    }
}

#[derive(Args, Debug)]
pub struct AmendArgs {
    #[arg(long)]
    pub account_id: String,
    #[arg(long)]
    pub order_id: String,
    #[arg(long)]
    pub qty: f64,
    #[arg(long)]
    pub limit_price: Option<f64>,
}

impl AmendArgs {
    pub fn into_request(self) -> AmendOrderRequest {
        AmendOrderRequest {
            account_id: self.account_id,
            order_id: self.order_id,
            qty: self.qty,
            limit_price: self.limit_price,
        }
    }
}
