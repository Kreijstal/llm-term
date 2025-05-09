mod shell;
mod model;
mod openrouter_client; // NEW: Add the openrouter_client module

use std::collections::HashMap;
use std::io::{self, Write};
use std::fs;
use std::process::Command as ProcessCommand;
use serde::{Deserialize, Serialize};
use clap::{Command, Arg};
use colored::*;
use std::path::PathBuf;
use shell::Shell;
use crate::model::Model;
use crate::openrouter_client::fetch_openrouter_models; // NEW: Import the fetch function

#[derive(Serialize, Deserialize)]
struct Config {
    model: Model,
    max_tokens: i32
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let matches = Command::new("llm-term")
        .version("1.0")
        .author("dh1101")
        .about("Generate terminal commands using OpenAI, OpenRouter, or local Ollama models")
        .arg(Arg::new("prompt")
            .help("The prompt describing the desired command")
            .required(false)
            .index(1))
        .arg(Arg::new("config")
            .short('c')
            .long("config")
            .help("Run configuration setup")
            .action(clap::ArgAction::SetTrue))
        .arg(
            Arg::new("disable-cache")
                .long("disable-cache")
                .help("Disable cache and always query the LLM")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let config_path = get_default_config_path().expect("Failed to get default config path");

    if matches.get_flag("config") {
        let config = create_config()?;
        let content = serde_json::to_string_pretty(&config)?;
        fs::write(&config_path, content)?;
        println!("{}", "Configuration saved successfully.".green());
        return Ok(());
    }

    let config = load_or_create_config(&config_path)?;

    let cache_path = get_cache_path()?;
    let mut cache = load_cache(&cache_path)?;

    if let Some(prompt) = matches.get_one::<String>("prompt") {
        let disable_cache = matches.get_flag("disable-cache");

        if !disable_cache {
            if let Some(cached_command) = cache.get(prompt) {
                println!("{}", "This command exists in cache".yellow());
                println!("{}", cached_command.cyan().bold());
                println!("{}", "Do you want to execute this command? (y/n)".yellow());

                let mut user_input = String::new();
                io::stdin().read_line(&mut user_input)?;

                if user_input.trim().to_lowercase() == "y" {
                    execute_command(cached_command)?;
                } else {
                    println!("{}", "Do you want to invalidate the cache? (y/n)".yellow());
                    user_input.clear();
                    io::stdin().read_line(&mut user_input)?;

                    if user_input.trim().to_lowercase() == "y" {
                        cache.remove(prompt);
                        save_cache(&cache_path, &cache)?;
                        get_command_from_llm(&config, &mut cache, &cache_path, prompt)?;
                    } else {
                        println!("{}", "Command execution cancelled.".yellow());
                    }
                }
                return Ok(());
            } else {
                get_command_from_llm(&config, &mut cache, &cache_path, prompt)?;
            }
        } else {
            get_command_from_llm(&config, &mut cache, &cache_path, prompt)?;
        }
    } else {
        println!("{}", "Please provide a prompt or use --config to set up the configuration.".yellow());
    }

    Ok(())
}

fn get_default_config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or("Failed to get executable directory")?;
    Ok(exe_dir.join("config.json"))
}

fn load_or_create_config(path: &PathBuf) -> Result<Config, Box<dyn std::error::Error>> {
    if let Ok(content) = fs::read_to_string(path) {
        Ok(serde_json::from_str(&content)?)
    } else {
        let config = create_config()?;
        let content = serde_json::to_string_pretty(&config)?;
        fs::write(path, content)?;
        Ok(config)
    }
}

fn create_config() -> Result<Config, io::Error> {
    // This outer loop is for selecting the model provider
    let mut selected_model_enum;
    let mut model_context_length_opt;
    
    loop {
        println!(
            "{}",
            "Select model provider:\n 1 for OpenAI (gpt-4o-mini)\n 2 for OpenAI (gpt-4o)\n 3 for Ollama\n 4 for OpenRouter".cyan()
        );
        io::stdout().flush()?;
        let mut choice = String::new();
        io::stdin().read_line(&mut choice)?;

        match choice.trim() {
            "1" => {
                selected_model_enum = Model::OpenAiGpt4oMini;
                model_context_length_opt = Some(128_000);
                break;
            }
            "2" => {
                selected_model_enum = Model::OpenAiGpt4o;
                model_context_length_opt = Some(128_000);
                break;
            }
            "3" => {
                println!("{}", "Enter Ollama model name (default: llama3.1):".cyan());
                io::stdout().flush()?;
                let mut ollama_model_name = String::new();
                io::stdin().read_line(&mut ollama_model_name)?;
                let trimmed_name = ollama_model_name.trim();
                let model_id = if trimmed_name.is_empty() { "llama3.1".to_string() } else { trimmed_name.to_string() };
                selected_model_enum = Model::Ollama(model_id);
                model_context_length_opt = None;
                break;
            }
            "4" => { // OpenRouter selection
                let openrouter_api_key = match std::env::var("OPENROUTER_API_KEY") {
                    Ok(key) => key,
                    Err(_) => {
                        println!("{}", "OPENROUTER_API_KEY environment variable not set.".red());
                        println!("{}", "Please set it and try again, or choose another provider.".yellow());
                        continue; // Restart provider selection
                    }
                };

                println!("{}", "Fetching models from OpenRouter...".yellow());
                match fetch_openrouter_models(&openrouter_api_key) {
                    Ok(mut available_models) => {
                        if available_models.is_empty() {
                            println!("{}", "No suitable models found on OpenRouter (they might be missing context length info). Try manual entry or another provider.".yellow());
                            continue; // Restart provider selection
                        }
                        // Filter for models with known context length for simplicity
                        available_models.retain(|m| m.context_length.is_some());

                        println!("{}", "Select an OpenRouter model:".cyan());
                        for (idx, model_info) in available_models.iter().enumerate() {
                            println!(" {}. {} (Context: {} tokens)", idx + 1, model_info.id, model_info.context_length.unwrap_or(0));
                        }

                        // Inner loop for selecting a specific OpenRouter model from the list
                        loop {
                            print!("{}", "Enter model number: ".cyan());
                            io::stdout().flush()?;
                            let mut model_choice_str = String::new();
                            io::stdin().read_line(&mut model_choice_str)?;
                            match model_choice_str.trim().parse::<usize>() {
                                Ok(num) if num > 0 && num <= available_models.len() => {
                                    let chosen_or_model = available_models[num - 1].clone();
                                    selected_model_enum = Model::OpenRouter { model_name: chosen_or_model.id };
                                    model_context_length_opt = chosen_or_model.context_length;
                                    break;
                                }
                                _ => println!("{}", "Invalid selection. Please enter a valid number from the list.".red()),
                            }
                        }
                    }
                    Err(e) => {
                        println!("{}", format!("Error fetching OpenRouter models: {}", e).red());
                        println!("{}", "Falling back to manual OpenRouter model entry.".yellow());
                        println!("{}", "Enter OpenRouter model identifier manually (e.g., mistralai/mistral-7b-instruct):".cyan());
                        io::stdout().flush()?;
                        let mut or_model_name_manual = String::new();
                        io::stdin().read_line(&mut or_model_name_manual)?;
                        let trimmed_name_manual = or_model_name_manual.trim();
                        if trimmed_name_manual.is_empty() {
                            println!("{}", "OpenRouter model name cannot be empty if entered manually. Retrying provider selection.".red());
                            continue;
                        }
                        selected_model_enum = Model::OpenRouter { model_name: trimmed_name_manual.to_string() };
                        model_context_length_opt = None;
                        break;
                    }
                }
            }
            _ => {
                println!("{}", "Invalid choice. Please try again.".red());
                continue;
            }
        }
    };

    // Prompt for max_tokens, using the fetched context length if available
    let default_max_tokens_value = 150;
    let max_tokens_upper_bound = model_context_length_opt.unwrap_or(4096);

    let max_tokens_prompt = format!(
        "Enter max tokens for completion (1-{}, default {}{}): ",
        max_tokens_upper_bound,
        default_max_tokens_value,
        model_context_length_opt.map_or("".to_string(), |cl| format!(", model context: {}", cl))
    );

    let final_max_tokens = loop {
        print!("{}", max_tokens_prompt.cyan());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let trimmed_input = input.trim();

        if trimmed_input.is_empty() {
            break default_max_tokens_value;
        }
        match trimmed_input.parse::<i32>() {
            Ok(tokens) => {
                if tokens > 0 && tokens <= max_tokens_upper_bound {
                    break tokens;
                } else if tokens > max_tokens_upper_bound {
                    println!("{}", format!("Max tokens for completion cannot exceed model context limit of {} (or chosen upper bound).", max_tokens_upper_bound).red());
                } else {
                    println!("{}", "Max tokens must be a positive number.".red());
                }
            }
            Err(_) => {
                println!("{}", format!("Invalid input. Please enter a number between 1 and {}.", max_tokens_upper_bound).red());
            }
        }
    };

    Ok(Config {
        model: selected_model_enum,
        max_tokens: final_max_tokens,
    })
}

fn get_cache_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let exe_path = std::env::current_exe()?;
    let exe_dir = exe_path.parent().ok_or("Failed to get executable directory")?;
    Ok(exe_dir.join("cache.json"))
}

fn load_cache(path: &PathBuf) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    if let Ok(content) = fs::read_to_string(path) {
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(HashMap::new())
    }
}

fn save_cache(path: &PathBuf, cache: &HashMap<String, String>) -> Result<(), Box<dyn std::error::Error>> {
    let content = serde_json::to_string_pretty(&cache)?;
    fs::write(path, content)?;
    Ok(())
}

fn get_command_from_llm(
    config: &Config,
    cache: &mut HashMap<String, String>,
    cache_path: &PathBuf,
    prompt: &String,
) -> Result<(), Box<dyn std::error::Error>> {
    match config.model.llm_get_command(config, prompt.as_str()) {
        Ok(Some(command)) => {
            println!("{}", &command.cyan().bold());
            println!("{}", "Do you want to execute this command? (y/n)".yellow());

            let mut user_input = String::new();
            io::stdin().read_line(&mut user_input)?;

            if user_input.trim().to_lowercase() == "y" {
                execute_command(&command)?;
            } else {
                println!("{}", "Command execution cancelled.".yellow());
            }

            cache.insert(prompt.clone(), command.clone());
            save_cache(cache_path, cache)?;
        },
        Ok(None) => println!("{}", "No command could be generated.".yellow()),
        Err(e) => eprintln!("{}", format!("Error getting command from LLM: {}", e).red()),
    }

    Ok(())
}

fn execute_command(command: &str) -> Result<(), Box<dyn std::error::Error>> {
    let (shell_cmd, shell_arg) = Shell::detect().to_shell_command_and_command_arg();

    match ProcessCommand::new(shell_cmd).arg(shell_arg).arg(&command).output() {
        Ok(output) => {
            println!("{}", "Command output:".green().bold());
            io::stdout().write_all(&output.stdout)?;
            io::stderr().write_all(&output.stderr)?;
        }
        Err(e) => eprintln!("{}", format!("Failed to execute command: {}", e).red()),
    }

    Ok(())
}