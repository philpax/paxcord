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

/// Trait for currency conversion backends
#[serenity::async_trait]
trait CurrencyBackend: Send + Sync {
    /// Fetch exchange rates for a base currency
    async fn fetch_rates(&self, base: &str) -> anyhow::Result<HashMap<String, f64>>;
}

/// Production backend using ExchangeRate-API
struct ExchangeRateApiBackend {
    client: reqwest::Client,
}

impl ExchangeRateApiBackend {
    fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[serenity::async_trait]
impl CurrencyBackend for ExchangeRateApiBackend {
    async fn fetch_rates(&self, base: &str) -> anyhow::Result<HashMap<String, f64>> {
        let url = format!("https://open.er-api.com/v6/latest/{}", base);

        let response = self
            .client
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

        Ok(data.conversion_rates)
    }
}

/// Currency converter with caching
pub struct CurrencyConverter {
    cache: Mutex<HashMap<String, RateCache>>,
    backend: Arc<dyn CurrencyBackend>,
}

impl CurrencyConverter {
    /// Create a new converter with the production backend
    pub fn new() -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            backend: Arc::new(ExchangeRateApiBackend::new()),
        }
    }

    #[cfg(test)]
    fn with_backend(backend: Arc<dyn CurrencyBackend>) -> Self {
        Self {
            cache: Mutex::new(HashMap::new()),
            backend,
        }
    }

    /// Convert an amount from one currency to another
    pub async fn convert(&self, from: &str, to: &str, amount: f64) -> anyhow::Result<f64> {
        let from = from.to_uppercase();
        let to = to.to_uppercase();

        if from == to {
            return Ok(amount);
        }

        let rate = self.get_conversion_rate(&from, &to).await?;
        Ok(amount * rate)
    }

    /// Get the conversion rate from one currency to another
    pub async fn rate(&self, from: &str, to: &str) -> anyhow::Result<f64> {
        let from = from.to_uppercase();
        let to = to.to_uppercase();

        if from == to {
            return Ok(1.0);
        }

        self.get_conversion_rate(&from, &to).await
    }

    /// Clear the cache
    pub async fn clear_cache(&self) {
        self.cache.lock().await.clear();
    }

    /// Get conversion rate from one currency to another
    async fn get_conversion_rate(&self, from: &str, to: &str) -> anyhow::Result<f64> {
        let now = SystemTime::now();

        // Try direct lookup first (from -> to)
        if let Some(rate) = self.check_cache(from, to, now).await {
            return Ok(rate);
        }

        // Try reverse lookup (to -> from) and invert
        if let Some(rate) = self.check_cache(to, from, now).await {
            return Ok(1.0 / rate);
        }

        // Check if we need to respect rate limiting
        let should_fetch = self.should_fetch_from_api(from, now).await;

        if should_fetch {
            // Fetch new rates from API
            self.fetch_and_cache_rates(from).await?;

            // Update timestamp after fetch since time has passed
            let now = SystemTime::now();

            // Try direct lookup again
            if let Some(rate) = self.check_cache(from, to, now).await {
                return Ok(rate);
            }
        } else {
            // Try to calculate through intermediate currencies
            if let Some(rate) = self.calculate_through_intermediates(from, to, now).await {
                return Ok(rate);
            }

            // If we still don't have it and haven't fetched recently, fetch anyway
            self.fetch_and_cache_rates(from).await?;

            // Update timestamp after fetch since time has passed
            let now = SystemTime::now();

            if let Some(rate) = self.check_cache(from, to, now).await {
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
    async fn check_cache(&self, from: &str, to: &str, now: SystemTime) -> Option<f64> {
        let cache = self.cache.lock().await;

        if let Some(entry) = cache.get(from) {
            // Check if cache is still valid (within 24 hours)
            if now.duration_since(entry.timestamp).ok()? < CACHE_DURATION {
                return entry.rates.get(to).copied();
            }
        }

        None
    }

    /// Check if we should fetch from API based on rate limiting
    async fn should_fetch_from_api(&self, from: &str, now: SystemTime) -> bool {
        let cache = self.cache.lock().await;

        if let Some(entry) = cache.get(from) {
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
        &self,
        from: &str,
        to: &str,
        now: SystemTime,
    ) -> Option<f64> {
        let cache = self.cache.lock().await;

        // Check if we have rates FROM the source currency
        if let Some(from_entry) = cache.get(from) {
            if now.duration_since(from_entry.timestamp).ok()? < CACHE_DURATION {
                // Try two-hop conversion: from -> intermediate -> to
                for (intermediate, from_to_intermediate_rate) in &from_entry.rates {
                    if let Some(intermediate_entry) = cache.get(intermediate) {
                        if now.duration_since(intermediate_entry.timestamp).ok()? < CACHE_DURATION {
                            if let Some(intermediate_to_to_rate) = intermediate_entry.rates.get(to)
                            {
                                return Some(from_to_intermediate_rate * intermediate_to_to_rate);
                            }
                        }
                    }
                }
            }
        }

        // Try reverse path: find an intermediate currency that has rates to both 'from' and 'to'
        for (_base, entry) in cache.iter() {
            if now.duration_since(entry.timestamp).ok()? < CACHE_DURATION {
                if let (Some(base_to_from), Some(base_to_to)) =
                    (entry.rates.get(from), entry.rates.get(to))
                {
                    // Rate from 'from' to 'to' = (1 / base_to_from) * base_to_to
                    // This is: (from/base) = 1/(base/from), then (from/to) = (from/base) * (base/to)
                    return Some(base_to_to / base_to_from);
                }
            }
        }

        None
    }

    /// Fetch exchange rates from API and cache them
    async fn fetch_and_cache_rates(&self, base: &str) -> anyhow::Result<()> {
        let now = SystemTime::now();
        let rates = self.backend.fetch_rates(base).await?;

        // Store in cache
        let mut cache = self.cache.lock().await;
        cache.insert(
            base.to_string(),
            RateCache {
                rates,
                timestamp: now,
                last_request: now,
            },
        );

        Ok(())
    }
}

impl Default for CurrencyConverter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn register(lua: &mlua::Lua) -> mlua::Result<()> {
    let currency = lua.create_table()?;
    let converter = Arc::new(CurrencyConverter::new());

    // Main conversion function
    currency.set(
        "convert",
        lua.create_async_function({
            let converter = converter.clone();
            move |_lua, (from, to, amount): (String, String, f64)| {
                let converter = converter.clone();
                async move {
                    converter
                        .convert(&from, &to, amount)
                        .await
                        .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
                }
            }
        })?,
    )?;

    // Function to get just the conversion rate
    currency.set(
        "rate",
        lua.create_async_function({
            let converter = converter.clone();
            move |_lua, (from, to): (String, String)| {
                let converter = converter.clone();
                async move {
                    converter
                        .rate(&from, &to)
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
            let converter = converter.clone();
            move |_lua, ()| {
                let converter = converter.clone();
                async move {
                    converter.clear_cache().await;
                    Ok(())
                }
            }
        })?,
    )?;

    lua.globals().set("currency", currency)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Mock backend for testing that returns predefined rates
    struct MockBackend {
        rates: Mutex<HashMap<String, HashMap<String, f64>>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                rates: Mutex::new(HashMap::new()),
            }
        }

        fn set_rates(&self, base: &str, rates: HashMap<String, f64>) {
            self.rates
                .try_lock()
                .unwrap()
                .insert(base.to_string(), rates);
        }
    }

    #[serenity::async_trait]
    impl CurrencyBackend for MockBackend {
        async fn fetch_rates(&self, base: &str) -> anyhow::Result<HashMap<String, f64>> {
            self.rates
                .lock()
                .await
                .get(base)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Mock backend: no rates for {}", base))
        }
    }

    fn create_test_converter() -> CurrencyConverter {
        let backend = Arc::new(MockBackend::new());
        CurrencyConverter::with_backend(backend)
    }

    #[tokio::test]
    #[serial]
    async fn test_check_cache_direct() {
        let converter = create_test_converter();
        let mut cache_lock = converter.cache.lock().await;

        // Set up cache with ZZA rates
        let mut zza_rates = HashMap::new();
        zza_rates.insert("ZZB".to_string(), 0.92);
        zza_rates.insert("ZZC".to_string(), 0.79);

        cache_lock.insert(
            "ZZA".to_string(),
            RateCache {
                rates: zza_rates,
                timestamp: SystemTime::now(),
                last_request: SystemTime::now(),
            },
        );

        drop(cache_lock);

        // Test direct cache lookup
        let result = converter.check_cache("ZZA", "ZZB", SystemTime::now()).await;
        assert!(result.is_some());
        assert!((result.unwrap() - 0.92).abs() < 0.001);
    }

    #[tokio::test]
    #[serial]
    async fn test_intermediate_calculation_two_hop() {
        let converter = create_test_converter();
        let mut cache_lock = converter.cache.lock().await;

        // Set up cache: XXA -> XXB and XXB -> XXC
        let mut xxa_rates = HashMap::new();
        xxa_rates.insert("XXB".to_string(), 0.92);

        let mut xxb_rates = HashMap::new();
        xxb_rates.insert("XXC".to_string(), 0.86);
        xxb_rates.insert("XXA".to_string(), 1.087);

        cache_lock.insert(
            "XXA".to_string(),
            RateCache {
                rates: xxa_rates,
                timestamp: SystemTime::now(),
                last_request: SystemTime::now(),
            },
        );

        cache_lock.insert(
            "XXB".to_string(),
            RateCache {
                rates: xxb_rates,
                timestamp: SystemTime::now(),
                last_request: SystemTime::now(),
            },
        );

        drop(cache_lock);

        // Test two-hop: XXA -> XXB -> XXC
        let result = converter
            .calculate_through_intermediates("XXA", "XXC", SystemTime::now())
            .await;
        assert!(result.is_some());
        // 0.92 * 0.86 = 0.7912
        assert!((result.unwrap() - 0.7912).abs() < 0.001);
    }

    #[tokio::test]
    #[serial]
    async fn test_intermediate_calculation_reverse_path() {
        let converter = create_test_converter();
        let mut cache_lock = converter.cache.lock().await;

        // Set up cache: XYB has rates to both XYA and XYC
        let mut xyb_rates = HashMap::new();
        xyb_rates.insert("XYA".to_string(), 1.087);
        xyb_rates.insert("XYC".to_string(), 0.86);

        cache_lock.insert(
            "XYB".to_string(),
            RateCache {
                rates: xyb_rates,
                timestamp: SystemTime::now(),
                last_request: SystemTime::now(),
            },
        );

        drop(cache_lock);

        // Test reverse path: XYA -> XYB -> XYC (through XYB having both)
        let result = converter
            .calculate_through_intermediates("XYA", "XYC", SystemTime::now())
            .await;
        assert!(result.is_some());
        // (0.86 / 1.087) = 0.791...
        assert!((result.unwrap() - (0.86 / 1.087)).abs() < 0.001);
    }

    #[tokio::test]
    #[serial]
    async fn test_intermediate_calculation_not_found() {
        let converter = create_test_converter();
        let mut cache_lock = converter.cache.lock().await;

        // Set up cache with no path from JPY to BRL
        let mut jpy_rates = HashMap::new();
        jpy_rates.insert("USD".to_string(), 0.0067);

        cache_lock.insert(
            "JPY".to_string(),
            RateCache {
                rates: jpy_rates,
                timestamp: SystemTime::now(),
                last_request: SystemTime::now(),
            },
        );

        drop(cache_lock);

        // Test no path available
        let result = converter
            .calculate_through_intermediates("JPY", "BRL", SystemTime::now())
            .await;
        assert!(result.is_none());
    }

    #[tokio::test]
    #[serial]
    async fn test_mock_backend_fetch() {
        let backend = Arc::new(MockBackend::new());
        let converter = CurrencyConverter::with_backend(backend.clone());

        // Set up mock rates
        let mut usd_rates = HashMap::new();
        usd_rates.insert("EUR".to_string(), 0.92);
        usd_rates.insert("GBP".to_string(), 0.79);
        backend.set_rates("USD", usd_rates);

        // This should use the mock backend, not hit the real API
        let result = converter.convert("USD", "EUR", 100.0).await;
        assert!(result.is_ok(), "Conversion failed: {:?}", result.err());
        assert!((result.unwrap() - 92.0).abs() < 0.1);
    }

    #[tokio::test]
    #[serial]
    async fn test_mock_backend_no_api_calls() {
        let converter = create_test_converter();

        // Don't set up any rates - this should fail without hitting the API
        let result = converter.convert("USD", "EUR", 100.0).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Mock backend: no rates for USD")
        );
    }
}
