use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = trader::cli::Cli::parse();
    let client = terminal_client::QuantdHttpClient::new(cli.base_url.clone(), cli.api_key.clone());

    match cli.command {
        trader::cli::Command::Tui => {
            terminal_tui::run(client)
                .await
                .map_err(std::io::Error::other)?;
        }
        trader::cli::Command::Quote { symbol } => {
            let quote = client
                .get_quote(&symbol)
                .await
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            println!("{}", trader::output::render_quote(&quote, cli.json)?);
        }
        trader::cli::Command::Orders { action } => match action {
            trader::cli::OrdersCommand::List { account_id } => {
                let orders = client
                    .get_orders(&account_id)
                    .await
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                println!("{}", trader::output::render_orders(&orders, cli.json)?);
            }
        },
        trader::cli::Command::Order { action } => match action {
            trader::cli::OrderCommand::Submit(body) => {
                let result = client
                    .submit_order(&body.into_request())
                    .await
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                println!("{}", trader::output::render_order_action(&result, cli.json)?);
            }
            trader::cli::OrderCommand::Cancel(args) => {
                let result = client
                    .cancel_order(&args.request(), &args.order_id)
                    .await
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                println!("{}", trader::output::render_order_action(&result, cli.json)?);
            }
            trader::cli::OrderCommand::Amend(body) => {
                let result = client
                    .amend_order(&body.into_request())
                    .await
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
                println!("{}", trader::output::render_order_action(&result, cli.json)?);
            }
        },
    }

    Ok(())
}
