use serde::Deserialize;

// Structs to represent the data from OpenRouter's /models endpoint
#[derive(Deserialize, Debug, Clone)]
pub struct OpenRouterModel {
    pub id: String, // e.g., "mistralai/mistral-7b-instruct"
    #[serde(rename = "context_length")]
    pub context_length: Option<i32>, // Total context window size
    // You can add more fields if needed, like description, pricing_info, etc.
}

#[derive(Deserialize, Debug)]
struct OpenRouterModelsResponse {
    data: Vec<OpenRouterModel>,
}

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
// Recommended headers for OpenRouter. Replace with your actual app URL if you have one.
const APP_URL: &str = "https://github.com/dh1011/llm-term";
const APP_TITLE: &str = "llm-term";

pub fn fetch_openrouter_models(api_key: &str) -> Result<Vec<OpenRouterModel>, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(format!("{}/{}", APP_TITLE, env!("CARGO_PKG_VERSION"))) // Good practice to set User-Agent
        .build()?;

    let response = client
        .get(OPENROUTER_MODELS_URL)
        .bearer_auth(api_key)
        .header("HTTP-Referer", APP_URL) // Recommended by OpenRouter
        .header("X-Title", APP_TITLE)     // Recommended by OpenRouter
        .send()?;

    let status = response.status();
    if !status.is_success() {
        let error_body = response.text().unwrap_or_else(|_| "No error body".to_string());
        return Err(format!(
            "Failed to fetch models from OpenRouter: {} - {}",
            status,
            error_body
        )
        .into());
    }

    let models_response: OpenRouterModelsResponse = response.json()?;
    let mut models = models_response.data;

    // Filter out models without an ID or context length for simplicity, or handle them differently
    models.retain(|model| model.context_length.is_some() && !model.id.is_empty());

    // Sort models by ID for consistent display
    models.sort_by(|a, b| a.id.cmp(&b.id));

    Ok(models)
}