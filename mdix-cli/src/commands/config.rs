
use clap::{Args, Subcommand};
use crate::commands::{handle_error, GlobalOpts};
use crate::config::ConfigManager;
use crate::output::{json_output, printer, table};

#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub subcommand: ConfigSubcommand,
}

#[derive(Subcommand)]
pub enum ConfigSubcommand {
    /// Get a config value
    Get(ConfigGetArgs),
    /// Set a config value
    Set(ConfigSetArgs),
    /// List all config values
    List,
    /// Reset config to defaults
    Reset(ConfigResetArgs),
}

#[derive(Args)]
pub struct ConfigGetArgs {
    /// Config key to read
    pub key: String,
}

#[derive(Args)]
pub struct ConfigSetArgs {
    /// Config key to update
    pub key:   String,
    /// New value
    pub value: String,
}

#[derive(Args)]
pub struct ConfigResetArgs {
    /// Specific key to reset (resets all keys if omitted)
    pub key: Option<String>,
}

pub fn run(args: ConfigArgs, global: &GlobalOpts) -> i32 {
    match args.subcommand {
        ConfigSubcommand::Get(a)   => run_get(a, global),
        ConfigSubcommand::Set(a)   => run_set(a, global),
        ConfigSubcommand::List     => run_list(global),
        ConfigSubcommand::Reset(a) => run_reset(a, global),
    }
}

fn run_get(args: ConfigGetArgs, global: &GlobalOpts) -> i32 {
    match ConfigManager::get_value(&args.key) {
        Ok(value) => {
            if global.json {
                json_output::print_result(serde_json::json!({
                    "key": args.key,
                    "value": value
                }));
            } else {
                println!("{}", value);
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}

fn run_set(args: ConfigSetArgs, global: &GlobalOpts) -> i32 {
    match ConfigManager::set_value(&args.key, &args.value) {
        Ok(()) => {
            if global.json {
                json_output::print_result(serde_json::json!({
                    "key": args.key,
                    "value": args.value
                }));
            } else if !global.quiet {
                printer::success(&format!("Set {} = {}", args.key, args.value));
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}

fn run_list(global: &GlobalOpts) -> i32 {
    let entries = ConfigManager::list_all();

    if global.json {
        let map: serde_json::Map<String, serde_json::Value> = entries
            .iter()
            .map(|(k, v, _)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        json_output::print_result(serde_json::Value::Object(map));
        return 0;
    }

    let config_path = ConfigManager::config_path();
    printer::info(&format!("Config file: {}", config_path.display()));
    println!();

    let rows: Vec<Vec<String>> = entries
        .iter()
        .map(|(key, value, is_default)| {
            let marker = if *is_default { "(default)" } else { "" };
            vec![key.clone(), value.clone(), marker.to_string()]
        })
        .collect();

    table::print_table(&["key", "value", ""], &rows);
    0
}

fn run_reset(args: ConfigResetArgs, global: &GlobalOpts) -> i32 {
    match ConfigManager::reset(args.key.as_deref()) {
        Ok(()) => {
            if global.json {
                json_output::print_result(serde_json::json!({ "reset": true }));
            } else if !global.quiet {
                match args.key {
                    Some(k) => printer::success(&format!("Reset {} to default", k)),
                    None    => printer::success("Reset all config to defaults"),
                }
            }
            0
        }
        Err(e) => handle_error(&e, global.json),
    }
}
