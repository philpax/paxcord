use std::collections::BTreeMap;

use serde::Deserialize;

use crate::config::Configuration;

/// Mirror of ananke's `/v1/models` entry — only `id` and the non-standard
/// `ananke_metadata` passthrough matter to paxcord. `object`/`created`/
/// `owned_by` exist on the wire but we don't use them, so serde drops
/// them on deserialization by default.
#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Model {
    pub id: String,
    /// Passthrough entries set via `[[service]] metadata.*` in ananke's
    /// config. Scripts look up keys like `metadata.discord_visible` here.
    /// Wire-format name is `ananke_metadata`; the Rust/Lua name is
    /// `metadata` for ergonomics on the consumer side.
    #[serde(default, rename(deserialize = "ananke_metadata"))]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<Model>,
}

pub struct Ai {
    pub client: async_openai::Client<async_openai::config::OpenAIConfig>,
    pub models: Vec<Model>,
}
impl Ai {
    pub async fn load(config: &Configuration) -> anyhow::Result<Self> {
        let client = async_openai::Client::with_config({
            let auth = &config.authentication;
            let mut config = async_openai::config::OpenAIConfig::default();
            if let Some(server) = auth.openai_api_server.as_deref() {
                config = config.with_api_base(server);
            }
            if let Some(key) = auth.openai_api_key.as_deref() {
                config = config.with_api_key(key);
            }
            config
        });

        let resp: ModelsResponse = client.models().list_byot().await?;
        Ok(Self {
            client,
            models: resp.data,
        })
    }
}
