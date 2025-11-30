-- scripts/commands.lua
-- Discord command definitions using the discord.register_command API

-- Currency choices for the convert command
local currencies = {
    {"USD", "US Dollar"},
    {"EUR", "Euro"},
    {"SEK", "Swedish Krona"},
    {"BRL", "Brazilian Real"},
    {"GBP", "British Pound"},
    {"PLN", "Polish Zloty"},
    {"JPY", "Japanese Yen"},
    {"AUD", "Australian Dollar"},
    {"CAD", "Canadian Dollar"},
    {"CHF", "Swiss Franc"},
    {"CNY", "Chinese Yuan"},
    {"INR", "Indian Rupee"},
    {"MXN", "Mexican Peso"},
    {"RUB", "Russian Ruble"},
    {"KRW", "South Korean Won"},
    {"TRY", "Turkish Lira"},
    {"ZAR", "South African Rand"},
    {"SGD", "Singapore Dollar"},
    {"HKD", "Hong Kong Dollar"},
    {"NOK", "Norwegian Krone"},
    {"NZD", "New Zealand Dollar"},
    {"THB", "Thai Baht"},
    {"AED", "UAE Dirham"},
    {"DKK", "Danish Krone"},
    {"IDR", "Indonesian Rupiah"},
}

-- Helper to create currency choices for command options
local function make_currency_choices()
    local choices = {}
    for _, currency in ipairs(currencies) do
        local code, name = currency[1], currency[2]
        table.insert(choices, {
            name = code .. " (" .. name .. ")",
            value = code
        })
    end
    return choices
end

-- Helper to create model choices from llm.models
local function make_model_choices()
    local choices = {}
    for _, model in ipairs(llm.models) do
        table.insert(choices, {
            name = model,
            value = model
        })
    end
    return choices
end

-- Register the /ask command (default hallucinate command)
discord.register_command({
    name = "ask",
    description = "Responds to the provided instruction",
    options = {
        {
            name = "model",
            description = "The model to use",
            type = "string",
            required = true,
            choices = make_model_choices()
        },
        {
            name = "prompt",
            description = "The prompt to send to the AI",
            type = "string",
            required = true
        },
        {
            name = "seed",
            description = "Random seed for deterministic output",
            type = "integer",
            required = false,
            min_value = 0,
            max_value = 2147483647
        }
    },
    execute = function(interaction)
        local model = interaction.options.model
        local prompt = interaction.options.prompt
        local seed = interaction.options.seed

        output("Generating...")

        local messages = {
            llm.system("You are a helpful assistant."),
            llm.user(prompt)
        }

        local response = stream_llm_response(messages, model, seed)

        output(response .. "\n\n-# Model: " .. model .. (seed and (" | Seed: " .. seed) or ""))
    end
})

-- Register the /convert command
discord.register_command({
    name = "convert",
    description = "Convert between currencies using live exchange rates",
    options = {
        {
            name = "amount",
            description = "The amount to convert",
            type = "number",
            required = true,
            min_value = 0.0
        },
        {
            name = "from",
            description = "The currency to convert from",
            type = "string",
            required = true,
            choices = make_currency_choices()
        },
        {
            name = "to",
            description = "The currency to convert to",
            type = "string",
            required = true,
            choices = make_currency_choices()
        }
    },
    execute = function(interaction)
        local amount = interaction.options.amount
        local from = interaction.options.from
        local to = interaction.options.to

        output("Converting...")

        local converted = currency.convert(from, to, amount)

        if converted then
            local rate = converted / amount
            output(string.format(
                "**%.2f %s** = **%.2f %s**\n-# Exchange rate: 1 %s = %.6f %s",
                amount, from, converted, to, from, rate, to
            ))
        else
            output("Failed to convert currency")
        end
    end
})

print("Commands registered successfully!")
