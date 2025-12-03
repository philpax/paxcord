-- scripts/commands.lua
-- Discord command definitions using the discord.register_command API

-- Currency data and choices for the convert command
local currency_data = {
	{ "USD", "US Dollar" },
	{ "EUR", "Euro" },
	{ "SEK", "Swedish Krona" },
	{ "BRL", "Brazilian Real" },
	{ "GBP", "British Pound" },
	{ "PLN", "Polish Zloty" },
	{ "JPY", "Japanese Yen" },
	{ "AUD", "Australian Dollar" },
	{ "CAD", "Canadian Dollar" },
	{ "CHF", "Swiss Franc" },
	{ "CNY", "Chinese Yuan" },
	{ "INR", "Indian Rupee" },
	{ "MXN", "Mexican Peso" },
	{ "RUB", "Russian Ruble" },
	{ "KRW", "South Korean Won" },
	{ "TRY", "Turkish Lira" },
	{ "ZAR", "South African Rand" },
	{ "SGD", "Singapore Dollar" },
	{ "HKD", "Hong Kong Dollar" },
	{ "NOK", "Norwegian Krone" },
	{ "NZD", "New Zealand Dollar" },
	{ "THB", "Thai Baht" },
	{ "AED", "UAE Dirham" },
	{ "DKK", "Danish Krone" },
	{ "IDR", "Indonesian Rupiah" },
}
local currency_choices = map(currency_data, function(currency)
	local code, name = currency[1], currency[2]
	return {
		name = code .. " (" .. name .. ")",
		value = code,
	}
end)

-- Register the /ask command (default hallucinate command)
discord.register_command {
	name = "ask",
	description = "Responds to the provided instruction",
	options = {
		{
			name = "model",
			description = "The model to use",
			type = "string",
			required = true,
			choices = map(llm.models, function(model)
				return {
					name = model,
					value = model,
				}
			end),
		},
		{
			name = "prompt",
			description = "The prompt to send to the AI",
			type = "string",
			required = true,
		},
		{
			name = "seed",
			description = "Random seed for deterministic output",
			type = "integer",
			required = false,
			min_value = 0,
			max_value = 2147483647,
		},
	},
	execute = function(interaction)
		local model = interaction.options.model
		local prompt = interaction.options.prompt
		local seed = interaction.options.seed

		output("Generating...")

		local messages = {
			llm.system("You are a helpful assistant."),
			llm.user(prompt),
		}

		local response = string.trim(stream_llm_response(messages, model, seed))

		output(response .. "\n\n-# Model: " .. model .. (seed and (" | Seed: " .. seed) or ""))
	end,
}

-- Register the /convert command
discord.register_command {
	name = "convert",
	description = "Convert between currencies using live exchange rates",
	options = {
		{
			name = "amount",
			description = "The amount to convert",
			type = "number",
			required = true,
			min_value = 0.0,
		},
		{
			name = "from",
			description = "The currency to convert from",
			type = "string",
			required = true,
			choices = currency_choices,
		},
		{
			name = "to",
			description = "The currency to convert to",
			type = "string",
			required = true,
			choices = currency_choices,
		},
	},
	execute = function(interaction)
		local amount = interaction.options.amount
		local from = interaction.options.from
		local to = interaction.options.to

		output("Converting...")

		local converted = currency.convert(amount, from, to)
		local rate = converted / amount
		local s = string.format(
			"**%.2f %s** = **%.2f %s**\n-# Exchange rate: 1 %s = %.6f %s",
			amount,
			from,
			converted,
			to,
			from,
			rate,
			to
		)
		output(s)
	end,
}

-- Register the /perchanceprompt command
discord.register_command {
	name = "perchanceprompt",
	description = "Generate random image prompts using https://perchance.org/image-synthesis-prompt-generator",
	options = {
		{
			name = "count",
			description = "Number of prompts to generate (default: 1)",
			type = "integer",
			required = false,
			min_value = 1,
			max_value = 10,
		},
		{
			name = "seed",
			description = "Random seed for deterministic output",
			type = "integer",
			required = false,
			min_value = 0,
			max_value = 4294967295, -- 2^32 - 1 (u32 max)
		},
	},
	execute = function(interaction)
		local count = interaction.options.count or 1
		local seed = interaction.options.seed or math.random(1, 2147483647) -- Use i32 max for Lua compatibility

		local generator = "output = {import:prompt_generator}"

		if count == 1 then
			local result = perchance.run(generator, seed)
			print(result)
		else
			for i = 0, count - 1 do
				local result = perchance.run(generator, seed + i)
				print((i + 1) .. ". " .. result)
			end
		end
	end,
}

-- Register the /paintsdxl command
discord.register_command {
	name = "paintsdxl",
	description = "Generate an image using Stable Diffusion XL via ComfyUI",
	options = {
		{
			name = "prompt",
			description = "The prompt describing the image to generate",
			type = "string",
			required = true,
		},
		{
			name = "negative",
			description = "Negative prompt (default: 'text, watermark, blurry')",
			type = "string",
			required = false,
		},
		{
			name = "seed",
			description = "Random seed for deterministic output (default: random)",
			type = "integer",
			required = false,
			min_value = 0,
			max_value = 2147483647,
		},
	},
	execute = function(interaction)
		local prompt = interaction.options.prompt
		local negative = interaction.options.negative or "text, watermark, blurry"
		local seed = interaction.options.seed or math.random(0, 2147483647)

		output("Connecting to ComfyUI...")

		-- Get client and object info (lazily cached)
		local client = get_comfy_client()
		local object_info = get_comfy_object_info()

		output("Building workflow...")

		-- Create the graph
		local g = comfy.graph(object_info)

		-- Build the SDXL workflow
		local c = g:CheckpointLoaderSimple("sd_xl_base_1.0.safetensors")
		local preview = g:PreviewImage(
			g:VAEDecode {
				vae = c.vae,
				samples = g:KSampler {
					model = c.model,
					seed = seed,
					steps = 20,
					cfg = 8.0,
					sampler_name = "euler",
					scheduler = "normal",
					positive = g:CLIPTextEncode { text = prompt, clip = c.clip },
					negative = g:CLIPTextEncode { text = negative, clip = c.clip },
					latent_image = g:EmptyLatentImage { width = 1024, height = 1024, batch_size = 1 },
					denoise = 1.0,
				},
			}
		)

		output("Generating image (seed: " .. seed .. ")...")

		-- Queue the workflow and wait for results
		local result = client:easy_queue(g)

		-- Get the images from the preview node
		local images = result[preview].images

		if #images > 0 then
			-- Attach each generated image
			for i, image_data in ipairs(images) do
				attach("image_" .. seed .. "_" .. i .. ".png", image_data)
			end
			output("Generated " .. #images .. " image(s)!\n\n-# Prompt: " .. prompt .. " | Seed: " .. seed)
		else
			output("No images were generated.")
		end
	end,
}
