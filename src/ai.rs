use std::{collections::BTreeMap, time::Duration};

use serde::Deserialize;

use crate::config::Configuration;

/// Initial delay before the first retry. Doubles on each subsequent
/// failure up to [`RETRY_MAX_DELAY`]. Total wait before giving up on
/// the defaults is ~30s (see [`RETRY_ATTEMPTS`]).
const RETRY_INITIAL_DELAY: Duration = Duration::from_millis(500);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(8);
const RETRY_ATTEMPTS: u32 = 6;

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

        let resp = fetch_models_with_backoff(&client).await?;
        Ok(Self {
            client,
            models: resp.data,
        })
    }
}

/// Poll `GET /v1/models` with exponential backoff. Ananke can take a
/// few seconds after systemd reports the unit as started before the
/// OpenAI proxy is accepting connections (schema migrations + spawning
/// persistent services), and `after = [ "ananke.service" ]` in the
/// paxcord systemd unit doesn't guarantee readiness. Rather than gate
/// paxcord's startup on a shell-level health probe, swallow the
/// expected early failures here.
async fn fetch_models_with_backoff(
    client: &async_openai::Client<async_openai::config::OpenAIConfig>,
) -> anyhow::Result<ModelsResponse> {
    let mut delay = RETRY_INITIAL_DELAY;
    for attempt in 1..=RETRY_ATTEMPTS {
        match client.models().list_byot::<ModelsResponse>().await {
            Ok(resp) => return Ok(resp),
            Err(err) if attempt == RETRY_ATTEMPTS => {
                return Err(anyhow::Error::new(err).context(format!(
                    "failed to fetch /v1/models after {RETRY_ATTEMPTS} attempts"
                )));
            }
            Err(err) => {
                eprintln!(
                    "paxcord: /v1/models attempt {attempt}/{RETRY_ATTEMPTS} failed ({err}); retrying in {:?}",
                    delay
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(RETRY_MAX_DELAY);
            }
        }
    }
    unreachable!("retry loop always returns inside the match")
}
