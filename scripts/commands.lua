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
		local seed = interaction.options.seed or math.random(1, 2147483647)

		output("Generating...")

		local messages = {
			llm.system("You are a helpful assistant."),
			llm.user(prompt),
		}

		local response = string.trim(stream_llm_response(messages, model, seed))

		output(response .. "\n\n-# Model: " .. model .. " | Seed: " .. seed)
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
			max_value = 2147483647,
		},
	},
	execute = function(interaction)
		local count = interaction.options.count or 1
		local seed = interaction.options.seed or math.random(1, 2147483647)

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

-- Model filenames: "Name [keyword] ^ARCH.ext" where [keyword] is optional
local model_data = {
	"ACertainModel ^SD1.ckpt",
	"ACertainThing ^SD1.ckpt",
	"AbyssOrangeMix2_sfw ^SD1.safetensors",
	"Analog [analog style] ^SD1.safetensors",
	"Anything v3.0 ^SD1.ckpt",
	"Cinematic Diffusion [syberart] ^SD1.ckpt",
	"Dreamlike Photoreal 2.0 ^SD1.safetensors",
	"Dreamlike [dreamlike art] ^SD1.ckpt",
	"Holosomnia Landscape [holosomnialandscape] ^SD1.ckpt",
	"Inkpunk v2 [nvinkpunk] ^SD1.ckpt",
	"Pastel Mix ^SD1.safetensors",
	"Stable v1.5 ^SD1.ckpt",
	"Van Gogh v2 [lvngvncnt] ^SD1.ckpt",
	"Vintedois v0.1 [estilovintedois] ^SD1.ckpt",
	"seek.art MEGA ^SD1.ckpt",
	"Stable v2.1 ^SD2.ckpt",
	"SDXL 1.0 ^SDXL.safetensors",
	"SDXL Turbo 1.0 ^SDXL.safetensors",
}

-- Default dimensions per architecture
local arch_defaults = {
	SD1 = { width = 512, height = 512 },
	SD2 = { width = 768, height = 768 },
	SDXL = { width = 1024, height = 1024 },
}

-- Parse a model filename into its components
local function parse_model_filename(filename)
	-- Extract keyword if present: [keyword]
	local keyword = filename:match("%[([^%]]+)%]")

	-- Extract architecture: ^ARCH
	local arch = filename:match("%^(%w+)")

	-- Extract display name: everything before [keyword] or ^ARCH
	local name = filename:match("^(.-)%s*%[") or filename:match("^(.-)%s*%^")

	return {
		filename = filename,
		name = name,
		arch = arch,
		keyword = keyword,
	}
end

-- Build model choices for command options
local model_choices = map(model_data, function(filename)
	local info = parse_model_filename(filename)
	return {
		name = info.name .. " (" .. info.arch .. ")",
		value = filename,
	}
end)

-- Helper to get model info by filename
local function get_model_info(filename)
	return parse_model_filename(filename)
end

-- Helper to get random model
local function get_random_model()
	local idx = math.random(1, #model_data)
	return parse_model_filename(model_data[idx])
end

-- Shared function to generate images
local function generate_image(prompt, negative, seed, model_info, width, height)
	-- Apply prompt keyword if model has one
	local final_prompt = prompt
	if model_info.keyword then
		final_prompt = prompt .. ", " .. model_info.keyword
	end

	-- Use architecture defaults if dimensions not specified
	local defaults = arch_defaults[model_info.arch]
	width = width or defaults.width
	height = height or defaults.height

	output("Connecting to ComfyUI...")

	-- Get client and object info (lazily cached)
	local client = get_comfy_client()
	local object_info = get_comfy_object_info()

	output("Building workflow...")

	-- Create the graph
	local g = comfy.graph(object_info)

	-- Build the workflow with selected model
	local c = g:CheckpointLoaderSimple(model_info.filename)
	local preview = g:PreviewImage(g:VAEDecode {
		vae = c.vae,
		samples = g:KSampler {
			model = c.model,
			seed = seed,
			steps = 20,
			cfg = 8.0,
			sampler_name = "euler",
			scheduler = "normal",
			positive = g:CLIPTextEncode { text = final_prompt, clip = c.clip },
			negative = g:CLIPTextEncode { text = negative, clip = c.clip },
			latent_image = g:EmptyLatentImage { width = width, height = height, batch_size = 1 },
			denoise = 1.0,
		},
	})

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
		output(
			string.format(
				"Prompt: %s | Model: %s | Size: %dx%d | Seed: %d",
				prompt,
				model_info.name,
				width,
				height,
				seed
			)
		)
	else
		output("No images were generated.")
	end
end

-- Register the /paint command
discord.register_command {
	name = "paint",
	description = "Generate an image via ComfyUI",
	options = {
		{
			name = "prompt",
			description = "The prompt describing the image to generate",
			type = "string",
			required = true,
		},
		{
			name = "model",
			description = "The model to use (default: random)",
			type = "string",
			required = false,
			choices = model_choices,
		},
		{
			name = "width",
			description = "Image width (default: based on model architecture)",
			type = "integer",
			required = false,
			min_value = 64,
			max_value = 2048,
		},
		{
			name = "height",
			description = "Image height (default: based on model architecture)",
			type = "integer",
			required = false,
			min_value = 64,
			max_value = 2048,
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
		local seed = interaction.options.seed or math.random(1, 2147483647)
		local model_info = interaction.options.model and get_model_info(interaction.options.model) or get_random_model()
		local width = interaction.options.width
		local height = interaction.options.height

		generate_image(prompt, negative, seed, model_info, width, height)
	end,
}

-- Default system prompt for askchorus
local askchorus_default_system = "You are a helpful assistant."

-- Register the /askchorus command
discord.register_command {
	name = "askchorus",
	description = "Ask multiple models the same prompt and stream all responses",
	options = {
		{
			name = "prompt",
			description = "The prompt to send to all models",
			type = "string",
			required = true,
		},
		{
			name = "system",
			description = "System prompt (default: '" .. askchorus_default_system .. "')",
			type = "string",
			required = false,
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
		local prompt = interaction.options.prompt
		local system_prompt = interaction.options.system or askchorus_default_system
		local seed = interaction.options.seed or math.random(1, 2147483647)
		local models = {
			"gpu:qwen3-4b-instruct",
			"gpu:gemma-3n-e4b-it",
			"gpu:gpt-oss-20b",
			"gpu:mistral-small-3.2-24b-instruct-2506",
			"gpu:gemma-3-27b-it",
			"gpu:gemma-3-27b-it-abliterated",
			"gpu:gemma-3-glitter-27b",
			"gpu:qwen3-30b-a3b-instruct-2507",
			"gpu:glm-4-32b-0414",
			"gpu:qwen3-32b",
		}

		local messages = {
			llm.system(system_prompt),
			llm.user(prompt),
		}

		local responses = {}

		local function format_output()
			local parts = { "**" .. prompt .. "** (seed: " .. seed .. ")\n" }
			for _, entry in ipairs(responses) do
				table.insert(parts, "\n`" .. entry.model .. "`: " .. string.trim(entry.response))
			end
			return table.concat(parts)
		end

		for _, model in ipairs(models) do
			table.insert(responses, { model = model, response = "" })
			output(format_output())

			llm.stream({
				messages = messages,
				model = model,
				seed = seed,
				callback = function(chunk)
					responses[#responses].response = chunk
					output(format_output())
					return true
				end,
			})
		end
	end,
}

-- Register the /paintperchance command
discord.register_command {
	name = "paintperchance",
	description = "Generate an image using a random prompt from Perchance",
	options = {
		{
			name = "model",
			description = "The model to use (default: random)",
			type = "string",
			required = false,
			choices = model_choices,
		},
		{
			name = "width",
			description = "Image width (default: based on model architecture)",
			type = "integer",
			required = false,
			min_value = 64,
			max_value = 2048,
		},
		{
			name = "height",
			description = "Image height (default: based on model architecture)",
			type = "integer",
			required = false,
			min_value = 64,
			max_value = 2048,
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
		local negative = interaction.options.negative or "text, watermark, blurry"
		local seed = interaction.options.seed or math.random(1, 2147483647)
		local model_info = interaction.options.model and get_model_info(interaction.options.model) or get_random_model()
		local width = interaction.options.width
		local height = interaction.options.height

		output("Generating prompt...")

		-- Generate a random prompt using Perchance
		local generator = "output = {import:prompt_generator}"
		local prompt = perchance.run(generator, seed)

		output("Generated prompt: " .. prompt)

		-- Generate the image using the generated prompt
		generate_image(prompt, negative, seed, model_info, width, height)
	end,
}

-- Reply handler for /ask - continues the conversation
discord.register_reply_handler("ask", function(chain)
	local model = chain.options.model
	local original_seed = chain.options.seed

	-- Require original parameters
	if not model then
		error("Original model parameter not available - cannot continue conversation")
	end
	if not original_seed then
		error("Original seed parameter not available - cannot continue conversation")
	end

	output("Continuing conversation...")

	-- Build the message history from the chain
	local messages = {
		llm.system("You are a helpful assistant."),
	}

	-- Add all messages from the chain
	for _, msg in ipairs(chain.messages) do
		if msg.is_bot then
			-- Bot messages are assistant responses
			-- Strip the metadata footer (everything after the last "\n\n-#")
			local content = msg.content
			local footer_pos = content:find("\n\n%-#[^\n]*$")
			if footer_pos then
				content = content:sub(1, footer_pos - 1)
			end
			table.insert(messages, llm.assistant(content))
		else
			-- User messages
			table.insert(messages, llm.user(msg.content))
		end
	end

	-- Stream the response
	local response = string.trim(stream_llm_response(messages, model, original_seed))

	output(response .. "\n\n-# Model: " .. model .. " | Seed: " .. original_seed .. " | Continued conversation")
end)

-- Reply handler for /paint - regenerates image with new prompt
discord.register_reply_handler("paint", function(chain)
	-- Get the original options - require model
	local original_model = chain.options.model
	if not original_model then
		error("Original model parameter not available - cannot regenerate image")
	end

	local original_width = chain.options.width
	local original_height = chain.options.height
	local original_negative = chain.options.negative or "text, watermark, blurry"

	-- Get the new prompt from the latest user message
	local new_prompt = nil
	for i = #chain.messages, 1, -1 do
		if not chain.messages[i].is_bot then
			new_prompt = chain.messages[i].content
			break
		end
	end

	if not new_prompt or new_prompt == "" then
		output("Please provide a prompt in your reply.")
		return
	end

	-- Generate a new seed for variation
	local new_seed = math.random(1, 2147483647)

	-- Get model info from the original model
	local model_info = get_model_info(original_model)

	-- Generate the image with the new prompt
	generate_image(new_prompt, original_negative, new_seed, model_info, original_width, original_height)
end)
