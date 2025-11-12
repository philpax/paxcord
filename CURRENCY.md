# Currency Conversion Feature

This document describes the currency conversion functionality added to paxcord.

## Features

### 1. Discord Slash Command: `/convert`

A Discord slash command for quick currency conversions with an easy-to-use interface.

**Parameters:**
- `amount` (Number, required): The amount to convert (must be ≥ 0)
- `from` (String, required): Source currency (dropdown with 25 popular currencies)
- `to` (String, required): Target currency (dropdown with 25 popular currencies)

**Supported Currencies:**
- USD (US Dollar)
- EUR (Euro)
- SEK (Swedish Krona)
- BRL (Brazilian Real)
- GBP (British Pound)
- PLN (Polish Zloty)
- JPY (Japanese Yen)
- AUD (Australian Dollar)
- CAD (Canadian Dollar)
- CHF (Swiss Franc)
- CNY (Chinese Yuan)
- INR (Indian Rupee)
- MXN (Mexican Peso)
- RUB (Russian Ruble)
- KRW (South Korean Won)
- TRY (Turkish Lira)
- ZAR (South African Rand)
- SGD (Singapore Dollar)
- HKD (Hong Kong Dollar)
- NOK (Norwegian Krone)
- NZD (New Zealand Dollar)
- THB (Thai Baht)
- AED (UAE Dirham)
- DKK (Danish Krone)
- IDR (Indonesian Rupiah)

**Example:**
```
/convert amount:100 from:USD to:EUR
```

**Output:**
```
**100.00 USD** = **91.90 EUR**

Exchange rate: 1 USD = 0.919000 EUR

Rates provided by ExchangeRate-API
```

### 2. Lua Extension: `currency` module

For programmatic access within Lua scripts, three functions are available:

#### `currency.convert(from, to, amount, api_key?)`

Convert an amount from one currency to another.

**Parameters:**
- `from` (string): Source currency code (e.g., "USD")
- `to` (string): Target currency code (e.g., "EUR")
- `amount` (number): Amount to convert
- `api_key` (string, optional): ExchangeRate-API key (uses free endpoint if omitted)

**Returns:** Converted amount (number)

**Example:**
```lua
local result = currency.convert("USD", "EUR", 100)
output("100 USD = " .. result .. " EUR")
```

#### `currency.rate(from, to, api_key?)`

Get the exchange rate between two currencies.

**Parameters:**
- `from` (string): Source currency code
- `to` (string): Target currency code
- `api_key` (string, optional): ExchangeRate-API key

**Returns:** Exchange rate (number)

**Example:**
```lua
local rate = currency.rate("USD", "EUR")
output("1 USD = " .. rate .. " EUR")
```

#### `currency.clear_cache()`

Clear the cached exchange rates. Useful for testing or forcing fresh data.

**Example:**
```lua
currency.clear_cache()
```

## Implementation Details

### Caching Strategy

The currency extension implements intelligent caching to minimize API requests while respecting rate limits:

1. **24-Hour Cache:** Exchange rates are cached for 24 hours (matching the API's update frequency)
2. **Rate Limiting:** Direct API requests are limited to once per hour per currency
3. **Intermediate Calculation:** If a direct rate isn't cached and we're within the 1-hour rate limit, the system attempts to calculate the conversion through intermediate currencies already in the cache

### Cache Calculation Algorithm

When a direct conversion rate isn't available and we're respecting rate limits, the system tries:

1. **Direct lookup:** `from → to`
2. **Reverse lookup:** `to → from` (then inverts the rate)
3. **Intermediate paths:**
   - `from → intermediate → to`
   - Through any cached currency that has rates to both `from` and `to`

This approach significantly reduces API calls while maintaining accuracy.

### API Usage

The implementation uses the ExchangeRate-API free tier:
- **Endpoint:** `https://open.er-api.com/v6/latest/{currency}`
- **Rate Limits:** Automatically managed by caching strategy
- **Attribution:** Included in Discord command responses

## Files Added/Modified

### New Files
- `src/commands/execute/extensions/currency.rs` - Lua extension for currency conversion
- `src/commands/currency.rs` - Discord slash command handler
- `CURRENCY.md` - This documentation

### Modified Files
- `Cargo.toml` - Added `reqwest` and `serde_json` dependencies
- `src/commands/execute/extensions/mod.rs` - Registered currency extension
- `src/commands/mod.rs` - Added currency command module
- `src/main.rs` - Registered currency command handler
- `src/util.rs` - Added `value_to_number()` helper function

## Testing

To test the currency conversion:

### Discord Command
1. Deploy the bot with the new changes
2. Use `/convert amount:100 from:USD to:EUR`
3. Verify the response includes converted amount and exchange rate

### Lua Extension
1. Use the `/execute` command with Lua code:
```lua
local result = currency.convert("USD", "EUR", 100)
output(string.format("100 USD = %.2f EUR", result))
```

## Attribution

Exchange rates provided by [ExchangeRate-API](https://www.exchangerate-api.com) (free tier).
