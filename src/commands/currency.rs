use serenity::all::{
    CommandInteraction, CommandOptionType, CreateCommand, CreateCommandOption, Http,
};
use serenity::model::application::Command;

use crate::{commands::CommandHandler, util};

pub struct Handler {
    discord_config: crate::config::Discord,
}

impl Handler {
    pub fn new(discord_config: crate::config::Discord) -> Self {
        Self { discord_config }
    }
}

// Top 25 most important currencies for the dropdown
const CURRENCIES: &[(&str, &str)] = &[
    ("USD", "US Dollar"),
    ("EUR", "Euro"),
    ("SEK", "Swedish Krona"),
    ("BRL", "Brazilian Real"),
    ("GBP", "British Pound"),
    ("PLN", "Polish Zloty"),
    ("JPY", "Japanese Yen"),
    ("AUD", "Australian Dollar"),
    ("CAD", "Canadian Dollar"),
    ("CHF", "Swiss Franc"),
    ("CNY", "Chinese Yuan"),
    ("INR", "Indian Rupee"),
    ("MXN", "Mexican Peso"),
    ("RUB", "Russian Ruble"),
    ("KRW", "South Korean Won"),
    ("TRY", "Turkish Lira"),
    ("ZAR", "South African Rand"),
    ("SGD", "Singapore Dollar"),
    ("HKD", "Hong Kong Dollar"),
    ("NOK", "Norwegian Krone"),
    ("NZD", "New Zealand Dollar"),
    ("THB", "Thai Baht"),
    ("AED", "UAE Dirham"),
    ("DKK", "Danish Krone"),
    ("IDR", "Indonesian Rupiah"),
];

#[serenity::async_trait]
impl CommandHandler for Handler {
    fn name(&self) -> &str {
        "convert"
    }

    async fn register(&self, http: &Http) -> anyhow::Result<()> {
        let mut from_option = CreateCommandOption::new(
            CommandOptionType::String,
            "from",
            "The currency to convert from.",
        )
        .required(true);

        let mut to_option = CreateCommandOption::new(
            CommandOptionType::String,
            "to",
            "The currency to convert to.",
        )
        .required(true);

        // Add currency choices to both dropdowns
        for (code, name) in CURRENCIES {
            let display = format!("{} ({})", code, name);
            from_option = from_option.add_string_choice(display.clone(), *code);
            to_option = to_option.add_string_choice(display, *code);
        }

        Command::create_global_command(
            http,
            CreateCommand::new(self.name())
                .description("Convert between currencies using live exchange rates.")
                .add_option(
                    CreateCommandOption::new(
                        CommandOptionType::Number,
                        "amount",
                        "The amount to convert.",
                    )
                    .required(true)
                    .min_number_value(0.0),
                )
                .add_option(from_option)
                .add_option(to_option),
        )
        .await?;
        Ok(())
    }

    async fn run(&self, http: &Http, cmd: &CommandInteraction) -> anyhow::Result<()> {
        let options = &cmd.data.options;

        let amount = util::get_value(options, "amount")
            .and_then(util::value_to_number)
            .ok_or_else(|| anyhow::anyhow!("Missing amount parameter"))?;

        let from = util::get_value(options, "from")
            .and_then(util::value_to_string)
            .ok_or_else(|| anyhow::anyhow!("Missing from currency parameter"))?;

        let to = util::get_value(options, "to")
            .and_then(util::value_to_string)
            .ok_or_else(|| anyhow::anyhow!("Missing to currency parameter"))?;

        // Defer the response as the API call might take a moment
        cmd.defer(http).await?;

        // Call the currency conversion API
        let client = reqwest::Client::new();
        let url = format!("https://open.er-api.com/v6/latest/{}", from);

        match client
            .get(&url)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                match response.json::<serde_json::Value>().await {
                    Ok(data) => {
                        if let Some(rate) = data["conversion_rates"][&to].as_f64() {
                            let converted = amount * rate;
                            let message = format!(
                                "**{:.2} {}** = **{:.2} {}**\n\nExchange rate: 1 {} = {:.6} {}\n\n_Rates provided by [ExchangeRate-API](https://www.exchangerate-api.com)_",
                                amount, from, converted, to, from, rate, to
                            );

                            cmd.edit_response(
                                http,
                                serenity::all::EditInteractionResponse::new().content(message),
                            )
                            .await?;
                        } else {
                            cmd.edit_response(
                                http,
                                serenity::all::EditInteractionResponse::new()
                                    .content(format!("❌ Could not find exchange rate for {} to {}", from, to)),
                            )
                            .await?;
                        }
                    }
                    Err(e) => {
                        cmd.edit_response(
                            http,
                            serenity::all::EditInteractionResponse::new()
                                .content(format!("❌ Failed to parse API response: {}", e)),
                        )
                        .await?;
                    }
                }
            }
            Ok(response) => {
                cmd.edit_response(
                    http,
                    serenity::all::EditInteractionResponse::new()
                        .content(format!("❌ API request failed with status: {}", response.status())),
                )
                .await?;
            }
            Err(e) => {
                cmd.edit_response(
                    http,
                    serenity::all::EditInteractionResponse::new()
                        .content(format!("❌ Failed to connect to API: {}", e)),
                )
                .await?;
            }
        }

        Ok(())
    }
}
