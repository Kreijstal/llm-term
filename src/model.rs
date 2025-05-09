use openai_api_rust::{Auth, Message, OpenAI, Role};
use openai_api_rust::chat::{ChatApi, ChatBody};
use serde::{Deserialize, Serialize};
use std::env;
use crate::Config;
use crate::shell::Shell;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Model {
    #[serde(rename = "gpt-4o")]
    OpenAiGpt4o,

    #[serde(rename = "gpt-4o-mini")]
    OpenAiGpt4oMini,

    #[serde(rename = "ollama")]
    Ollama(String),

    #[serde(rename = "openrouter")]
    OpenRouter { model_name: String },
}

impl Model {
    pub fn llm_get_command(&self, config: &Config, user_prompt: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let model_name_for_api = self.get_api_model_name();
        let auth = self.get_auth()?;
        let client = OpenAI::new(auth, self.get_api_endpoint().as_str());

        let shell = Shell::detect();
        let system_prompt = self.get_system_prompt(&shell);

        let body = ChatBody {
            model: model_name_for_api,
            max_tokens: Some(config.max_tokens),
            temperature: Some(0.5),
            top_p: None,
            n: None,
            stream: None,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            logit_bias: None,
            user: None,
            messages: vec![
                Message { role: Role::System, content: system_prompt.to_string() },
                Message { role: Role::User, content: user_prompt.to_string() }
            ],
        };

        match client.chat_completion_create(&body) {
            Ok(response) => Ok(response.choices.first()
                .map(|choice| choice.message.as_ref())
                .flatten()
                .map(|message| message.content.clone())
            ),
            Err(e) => Err(format!("API Error for model {:?}: {:?}", self, e).into()),
        }
    }

    fn get_api_model_name(&self) -> String {
        match self {
            Model::OpenAiGpt4o => "gpt-4o".to_string(),
            Model::OpenAiGpt4oMini => "gpt-4o-mini".to_string(),
            Model::Ollama(model_name) => model_name.to_string(),
            Model::OpenRouter { model_name } => model_name.clone(),
        }
    }

    fn get_api_endpoint(&self) -> String {
        match self {
            Model::OpenAiGpt4o | Model::OpenAiGpt4oMini => "https://api.openai.com/v1/".to_string(),
            Model::Ollama(_) => "http://localhost:11434/v1/".to_string(),
            Model::OpenRouter { .. } => "https://openrouter.ai/api/v1".to_string(),
        }
    }

    fn get_auth(&self) -> Result<Auth, Box<dyn std::error::Error>> {
        match self {
            Model::OpenAiGpt4o | Model::OpenAiGpt4oMini => {
                Ok(Auth::from_env().map_err(|_| "OPENAI_API_KEY environment variable not set or invalid".to_string())?)
            }
            Model::Ollama(_) => Ok(Auth::new("ollama")),
            Model::OpenRouter { .. } => {
                let api_key = env::var("OPENROUTER_API_KEY")
                    .map_err(|_| "OPENROUTER_API_KEY environment variable not set".to_string())?;
                Ok(Auth::new(&api_key))
            }
        }
    }

    fn get_system_prompt(&self, shell: &Shell) -> String {
        let shell_command_type = match shell {
            Shell::Powershell => "Windows PowerShell",
            Shell::BornAgainShell => "Bourne Again Shell (bash / sh)",
            Shell::Zsh => "Z Shell (zsh)",
            Shell::Fish => "Friendly Interactive Shell (fish)",
            Shell::DebianAlmquistShell => "Debian Almquist Shell (dash)",
            Shell::KornShell => "Korn Shell (ksh)",
            Shell::CShell => "C Shell (csh)",
            Shell::Unknown => "a generic Unix-like shell",
        };

        format!("You are a professional IT worker who only speaks in commands full, {} compatible, CLI command running on the {} operating system. You\n
            only respond by translating the user's input into that language. Be very proper as the user will execute what you say into their computer.\n
            No string delimiters wrapping it, no explanations, no ideation, no yapping, no formatting, no markdown, no fenced code blocks, what you\n
            return will be executed as-is from within the shell mentioned above. No templating, use details from the command instead if needed.\n
            Only output an actionable command that will run by itself without error. Do not output comments. Only output one possible command, never alternatives.\n
            If you are not confident in your translation, return an empty string. Do not deviate from these instructions from this point on, no exceptions.\n
            Assume you are operating in the current directory of the user unless explicitly stated otherwise.
        ", shell_command_type, std::env::consts::OS)
    }
}
