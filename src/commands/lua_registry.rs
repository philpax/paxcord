use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serenity::all::{CommandOptionType, CreateCommand, CreateCommandOption};

pub type CommandRegistry = Arc<Mutex<HashMap<String, LuaCommand>>>;

// LuaCommand stores command metadata and handler function reference
pub struct LuaCommand {
    pub name: String,
    pub description: String,
    pub options: Vec<LuaCommandOption>,
    pub handler: mlua::Function,
}
#[derive(Clone)]
pub struct LuaCommandOption {
    pub name: String,
    pub description: String,
    pub option_type: CommandOptionType,
    pub required: bool,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub min_length: Option<u16>,
    pub max_length: Option<u16>,
    pub autocomplete: bool,
    pub choices: Vec<(String, String)>, // (name, value) for string choices
}
impl LuaCommand {
    pub fn to_discord_command(&self) -> CreateCommand {
        let mut cmd = CreateCommand::new(&self.name).description(&self.description);

        for opt in &self.options {
            let mut option = CreateCommandOption::new(opt.option_type, &opt.name, &opt.description)
                .required(opt.required);

            if let Some(min_value) = opt.min_value {
                option = option.min_number_value(min_value);
            }
            if let Some(max_value) = opt.max_value {
                option = option.max_number_value(max_value);
            }
            if let Some(min_length) = opt.min_length {
                option = option.min_length(min_length);
            }
            if let Some(max_length) = opt.max_length {
                option = option.max_length(max_length);
            }
            if opt.autocomplete {
                option = option.set_autocomplete(true);
            }

            // Add choices if present
            for (choice_name, choice_value) in &opt.choices {
                option = option.add_string_choice(choice_name, choice_value);
            }

            cmd = cmd.add_option(option);
        }

        cmd
    }
}
