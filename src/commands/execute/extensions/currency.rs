use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::Deserialize;
use tokio::sync::Mutex;

/// Cache entry for exchange rates from a base currency
#[derive(Clone, Debug)]
struct RateCache {
    /// Map of target currency code to exchange rate
    rates: HashMap<String, f64>,
    /// When this cache entry was fetched
    timestamp: SystemTime,
    /// When we last made an API request for this base currency
    last_request: SystemTime,
}

/// Global cache for exchange rates
type CurrencyCache = Arc<Mutex<HashMap<String, RateCache>>>;

/// Response from ExchangeRate-API
#[derive(Debug, Deserialize)]
struct ExchangeRateResponse {
    result: String,
    conversion_rates: HashMap<String, f64>,
    #[serde(rename = "time_last_update_unix")]
    _time_last_update: u64,
}

const CACHE_DURATION: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(60 * 60); // 1 hour

pub fn register(lua: &mlua::Lua) -> mlua::Result<()> {
    let currency = lua.create_table()?;
    let cache: CurrencyCache = Arc::new(Mutex::new(HashMap::new()));

    // Main conversion function
    currency.set(
        "convert",
        lua.create_async_function({
            let cache = cache.clone();
            move |_lua, (from, to, amount, api_key): (String, String, f64, Option<String>)| {
                let cache = cache.clone();
                async move {
                    let from = from.to_uppercase();
                    let to = to.to_uppercase();

                    if from == to {
                        return Ok(amount);
                    }

                    // Try to get the conversion rate
                    let rate = get_conversion_rate(&cache, &from, &to, api_key.as_deref()).await
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;

                    Ok(amount * rate)
                }
            }
        })?,
    )?;

    // Function to get just the conversion rate
    currency.set(
        "rate",
        lua.create_async_function({
            let cache = cache.clone();
            move |_lua, (from, to, api_key): (String, String, Option<String>)| {
                let cache = cache.clone();
                async move {
                    let from = from.to_uppercase();
                    let to = to.to_uppercase();

                    if from == to {
                        return Ok(1.0);
                    }

                    get_conversion_rate(&cache, &from, &to, api_key.as_deref())
                        .await
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
                }
            }
        })?,
    )?;

    // Function to clear the cache
    currency.set(
        "clear_cache",
        lua.create_async_function({
            let cache = cache.clone();
            move |_lua, ()| {
                let cache = cache.clone();
                async move {
                    cache.lock().await.clear();
                    Ok(())
                }
            }
        })?,
    )?;

    lua.globals().set("currency", currency)?;

    Ok(())
}

/// Get conversion rate from one currency to another
async fn get_conversion_rate(
    cache: &CurrencyCache,
    from: &str,
    to: &str,
    api_key: Option<&str>,
) -> anyhow::Result<f64> {
    let now = SystemTime::now();

    // Try direct lookup first (from -> to)
    if let Some(rate) = check_cache(cache, from, to, now).await {
        return Ok(rate);
    }

    // Try reverse lookup (to -> from) and invert
    if let Some(rate) = check_cache(cache, to, from, now).await {
        return Ok(1.0 / rate);
    }

    // Check if we need to respect rate limiting
    let should_fetch = should_fetch_from_api(cache, from, now).await;

    if should_fetch {
        // Fetch new rates from API
        fetch_and_cache_rates(cache, from, api_key).await?;

        // Try direct lookup again
        if let Some(rate) = check_cache(cache, from, to, now).await {
            return Ok(rate);
        }
    } else {
        // Try to calculate through intermediate currencies
        if let Some(rate) = calculate_through_intermediates(cache, from, to, now).await {
            return Ok(rate);
        }

        // If we still don't have it and haven't fetched recently, fetch anyway
        fetch_and_cache_rates(cache, from, api_key).await?;

        if let Some(rate) = check_cache(cache, from, to, now).await {
            return Ok(rate);
        }
    }

    Err(anyhow::anyhow!(
        "Unable to find conversion rate from {} to {}",
        from,
        to
    ))
}

/// Check if rate exists in cache and is still valid
async fn check_cache(
    cache: &CurrencyCache,
    from: &str,
    to: &str,
    now: SystemTime,
) -> Option<f64> {
    let cache_lock = cache.lock().await;

    if let Some(entry) = cache_lock.get(from) {
        // Check if cache is still valid (within 24 hours)
        if now.duration_since(entry.timestamp).ok()? < CACHE_DURATION {
            return entry.rates.get(to).copied();
        }
    }

    None
}

/// Check if we should fetch from API based on rate limiting
async fn should_fetch_from_api(cache: &CurrencyCache, from: &str, now: SystemTime) -> bool {
    let cache_lock = cache.lock().await;

    if let Some(entry) = cache_lock.get(from) {
        // Check if we've requested this currency within the last hour
        if let Ok(duration) = now.duration_since(entry.last_request) {
            return duration >= MIN_REQUEST_INTERVAL;
        }
    }

    // If no entry exists or time calculation fails, allow fetch
    true
}

/// Try to calculate conversion rate through intermediate currencies
async fn calculate_through_intermediates(
    cache: &CurrencyCache,
    from: &str,
    to: &str,
    now: SystemTime,
) -> Option<f64> {
    let cache_lock = cache.lock().await;

    // Get all valid cached currencies
    let mut valid_intermediates = Vec::new();

    for (base, entry) in cache_lock.iter() {
        if now.duration_since(entry.timestamp).ok()? < CACHE_DURATION {
            valid_intermediates.push((base.as_str(), entry));
        }
    }

    // Try to find a path: from -> intermediate -> to
    for (intermediate, from_entry) in &valid_intermediates {
        // Check if we can go from 'from' to 'intermediate'
        if let Some(from_to_intermediate) = from_entry.rates.get(to) {
            // We found it directly in the from_entry
            return Some(*from_to_intermediate);
        }

        // Check for from -> intermediate -> to
        if let Some(from_to_inter) = from_entry.rates.get(*intermediate) {
            // Now check if we can go from intermediate to 'to'
            if let Some(inter_entry) = cache_lock.get(*intermediate) {
                if now.duration_since(inter_entry.timestamp).ok()? < CACHE_DURATION {
                    if let Some(inter_to_to) = inter_entry.rates.get(to) {
                        return Some(from_to_inter * inter_to_to);
                    }
                }
            }
        }
    }

    // Try reverse path: check if any intermediate has rates to both 'from' and 'to'
    for (_intermediate, inter_entry) in &valid_intermediates {
        if let (Some(inter_to_from), Some(inter_to_to)) =
            (inter_entry.rates.get(from), inter_entry.rates.get(to))
        {
            // Rate from 'from' to 'to' = (1 / inter_to_from) * inter_to_to
            return Some(inter_to_to / inter_to_from);
        }
    }

    None
}

/// Fetch exchange rates from API and cache them
async fn fetch_and_cache_rates(
    cache: &CurrencyCache,
    base: &str,
    api_key: Option<&str>,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let now = SystemTime::now();

    let url = if let Some(key) = api_key {
        format!(
            "https://v6.exchangerate-api.com/v6/{}/latest/{}",
            key, base
        )
    } else {
        // Use the open/free endpoint without API key
        format!("https://open.er-api.com/v6/latest/{}", base)
    };

    let response = client
        .get(&url)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "API request failed with status: {}",
            response.status()
        ));
    }

    let data: ExchangeRateResponse = response.json().await?;

    if data.result != "success" {
        return Err(anyhow::anyhow!("API returned non-success result"));
    }

    // Store in cache
    let mut cache_lock = cache.lock().await;
    cache_lock.insert(
        base.to_string(),
        RateCache {
            rates: data.conversion_rates,
            timestamp: now,
            last_request: now,
        },
    );

    Ok(())
}
