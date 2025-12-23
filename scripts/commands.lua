-- scripts/commands.lua
-- Discord command definitions using the discord.register_command API

-- ============================================================================
-- Footer serialization - roundtrippable structured format
-- Format: -# @key=type:value|key2=type:value2|...
-- Types: s=string, i=integer, n=number, b=boolean
-- ============================================================================
local footer = {}

-- Serialize a params table to footer string
function footer.serialize(params)
	local parts = {}
	local keys = {}
	for k in pairs(params) do
		table.insert(keys, k)
	end
	table.sort(keys)

	for _, k in ipairs(keys) do
		local v = params[k]
		local encoded
		if type(v) == "string" then
			-- Escape special chars: \ | =
			encoded = "s:" .. v:gsub("\\", "\\\\"):gsub("|", "\\p"):gsub("=", "\\e")
		elseif type(v) == "number" then
			if math.floor(v) == v then
				encoded = "i:" .. tostring(math.floor(v))
			else
				encoded = "n:" .. tostring(v)
			end
		elseif type(v) == "boolean" then
			encoded = "b:" .. (v and "1" or "0")
		end
		if encoded then
			table.insert(parts, k .. "=" .. encoded)
		end
	end
	return "\n\n-# @" .. table.concat(parts, "|")
end

-- Deserialize footer string back to params table
function footer.deserialize(content)
	local data = content:match("\n\n%-# @(.+)$")
	if not data then
		return nil
	end

	local result = {}
	for pair in data:gmatch("[^|]+") do
		local key, type_val = pair:match("([^=]+)=(.+)")
		if key and type_val then
			local t, v = type_val:match("^(.):(.*)$")
			if t == "s" then
				-- Unescape: \\ -> \, \p -> |, \e -> =
				result[key] = v:gsub("\\e", "="):gsub("\\p", "|"):gsub("\\\\", "\\")
			elseif t == "i" or t == "n" then
				result[key] = tonumber(v)
			elseif t == "b" then
				result[key] = v == "1"
			end
		end
	end
	return result
end

-- Strip footer from content (for building message history)
function footer.strip(content)
	local footer_pos = content:find("\n\n%-# @[^\n]*$")
	if footer_pos then
		return content:sub(1, footer_pos - 1)
	end
	return content
end

-- ============================================================================
-- Command-specific defaults
-- ============================================================================
local ask = {}
ask.default_system = "You are a helpful assistant."

local paint = {}
paint.default_negative = "text, watermark, blurry"
paint.default_denoise = 0.8

-- ============================================================================
-- Currency data and choices for the convert command
-- ============================================================================
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
			name = "system",
			description = "System prompt (default: '" .. ask.default_system .. "')",
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
		local model = interaction.options.model
		local prompt = interaction.options.prompt
		local system = interaction.options.system or ask.default_system
		local seed = interaction.options.seed or math.random(1, 2147483647)

		output("Generating...")

		local messages = {
			llm.system(system),
			llm.user(prompt),
		}

		local response = string.trim(stream_llm_response(messages, model, seed))

		output(response .. footer.serialize({ model = model, seed = seed, system = system }))
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
	"Nova Orange XL v13.0 ^SDXL.safetensors",
	"Nova Anime XL IL v14.0 ^SDXL.safetensors",
	"Illustrious v2.0 ^SDXL.safetensors",
	"Z Image Turbo ^ZImage.safetensors",
}

-- Default dimensions per architecture
local arch_defaults = {
	SD1 = { width = 512, height = 512 },
	SD2 = { width = 768, height = 768 },
	SDXL = { width = 1024, height = 1024 },
	ZImage = { width = 1024, height = 1024 },
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
local function generate_image(prompt, negative, seed, model_info, width, height, source_image_url, denoise)
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

	-- Load model, clip, and vae based on architecture
	local model, clip, vae
	if model_info.arch == "ZImage" then
		model = g:UNETLoader {
			unet_name = "z_image_turbo_bf16.safetensors",
			weight_dtype = "default",
		}
		clip = g:CLIPLoader {
			clip_name = "qwen_3_4b.safetensors",
			device = "default",
			type = "lumina2",
		}
		vae = g:VAELoader("flux1_ae.safetensors")
	else
		local c = g:CheckpointLoaderSimple(model_info.filename)
		model, clip, vae = c.model, c.clip, c.vae
	end

	-- Create latent image (swap node based on source image presence)
	local latent_image
	if source_image_url then
		-- img2img: fetch image, upload to ComfyUI, load, scale, encode
		output("Downloading source image...")
		local image_data = fetch(source_image_url)
		local uploaded = client:upload_image("source.png", image_data)
		local loaded = g:LoadImage { image = uploaded.name }
		local scaled = g:ImageScale {
			image = loaded.image,
			width = width,
			height = height,
			upscale_method = "bicubic",
			crop = "disabled",
		}
		latent_image = g:VAEEncode { pixels = scaled, vae = vae }
		denoise = denoise or paint.default_denoise
	else
		-- txt2img: empty latent
		if model_info.arch == "ZImage" then
			latent_image = g:EmptySD3LatentImage { width = width, height = height, batch_size = 1 }
		else
			latent_image = g:EmptyLatentImage { width = width, height = height, batch_size = 1 }
		end
		denoise = 1.0
	end

	-- KSampler settings based on architecture
	local steps, cfg, scheduler
	if model_info.arch == "ZImage" then
		steps, cfg, scheduler = 9, 1.0, "simple"
	else
		steps, cfg, scheduler = 20, 8.0, "normal"
	end

	-- Build the workflow
	local preview = g:PreviewImage(g:VAEDecode {
		vae = vae,
		samples = g:KSampler {
			model = model,
			seed = seed,
			steps = steps,
			cfg = cfg,
			sampler_name = "euler",
			scheduler = scheduler,
			positive = g:CLIPTextEncode { text = final_prompt, clip = clip },
			negative = g:CLIPTextEncode { text = negative, clip = clip },
			latent_image = latent_image,
			denoise = denoise,
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
		output(footer.serialize({
			prompt = prompt,
			model = model_info.filename,
			width = width,
			height = height,
			seed = seed,
			denoise = denoise,
			img2img = source_image_url ~= nil,
		}))
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
		{
			name = "image",
			description = "Source image for img2img (upload)",
			type = "attachment",
			required = false,
		},
		{
			name = "image_url",
			description = "Source image URL for img2img",
			type = "string",
			required = false,
		},
		{
			name = "denoise",
			description = "Denoising strength (0.0-1.0, default: "
				.. paint.default_denoise
				.. " for img2img, 1.0 for txt2img)",
			type = "number",
			required = false,
			min_value = 0.0,
			max_value = 1.0,
		},
	},
	execute = function(interaction)
		local prompt = interaction.options.prompt
		local negative = interaction.options.negative or paint.default_negative
		local seed = interaction.options.seed or math.random(1, 2147483647)
		local model_info = interaction.options.model and get_model_info(interaction.options.model) or get_random_model()
		local width = interaction.options.width
		local height = interaction.options.height

		-- Get source image (prefer attachment over URL)
		local source_image_url = interaction.options.image or interaction.options.image_url
		local denoise = interaction.options.denoise

		generate_image(prompt, negative, seed, model_info, width, height, source_image_url, denoise)
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
	-- Try to get parameters from options first
	local model = chain.options.model
	local original_seed = chain.options.seed
	local system = chain.options.system

	-- If options not available, try to parse from first bot message footer
	if not model or not original_seed then
		for _, msg in ipairs(chain.messages) do
			if msg.is_bot then
				local parsed = footer.deserialize(msg.content)
				if parsed then
					model = model or parsed.model
					original_seed = original_seed or parsed.seed
					system = system or parsed.system
					break
				end
			end
		end
	end

	-- Require parameters
	if not model then
		error("Original model parameter not available - cannot continue conversation")
	end
	if not original_seed then
		error("Original seed parameter not available - cannot continue conversation")
	end

	-- Default system prompt if still not found
	system = system or ask.default_system

	output("Continuing conversation...")

	-- Build the message history from the chain
	local messages = {
		llm.system(system),
	}

	-- Add the original prompt (slash command options aren't in chain.messages)
	if chain.options.prompt then
		table.insert(messages, llm.user(chain.options.prompt))
	end

	-- Add all messages from the chain
	for _, msg in ipairs(chain.messages) do
		if msg.is_bot then
			-- Bot messages are assistant responses - strip footer
			table.insert(messages, llm.assistant(footer.strip(msg.content)))
		else
			-- User messages
			table.insert(messages, llm.user(msg.content))
		end
	end

	-- Stream the response
	local response = string.trim(stream_llm_response(messages, model, original_seed))

	output(response .. footer.serialize({ model = model, seed = original_seed, system = system }))
end)

-- Register the /translate command
discord.register_command {
	name = "translate",
	description = "Translate text to a target language",
	options = {
		{
			name = "text",
			description = "The text to translate",
			type = "string",
			required = true,
		},
		{
			name = "language",
			description = "The target language",
			type = "string",
			required = true,
			suggestions = map({
				"English",
				"Spanish",
				"French",
				"German",
				"Italian",
				"Portuguese",
				"Dutch",
				"Polish",
				"Swedish",
				"Danish",
				"Norwegian",
				"Russian",
				"Turkish",
				"Arabic",
				"Japanese",
				"Korean",
				"Chinese (Simplified)",
				"Thai",
				"Indonesian",
				"Hindi",
			}, function(lang)
				return { name = lang, value = lang }
			end),
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
		local text = interaction.options.text
		local language = interaction.options.language
		local seed = interaction.options.seed or math.random(1, 2147483647)
		local model = "gpu:qwen3-30b-a3b-instruct-2507"

		output("Translating...")

		local messages = {
			llm.system(
				"You are a translator. Translate the user's text to the specified language. Output only the translation, nothing else."
			),
			llm.user("Translate to " .. language .. ":\n\n" .. text),
		}

		local response = string.trim(stream_llm_response(messages, model, seed))

		output(response .. footer.serialize({ model = model, seed = seed, language = language }))
	end,
}

-- Reply handler for /paint - regenerates image with new prompt
discord.register_reply_handler("paint", function(chain)
	-- Try to get parameters from options first
	local original_model = chain.options.model
	local original_width = chain.options.width
	local original_height = chain.options.height
	local original_negative = chain.options.negative
	local original_denoise = nil
	local original_img2img = false

	-- If model not available, try to parse from first bot message footer
	if not original_model then
		for _, msg in ipairs(chain.messages) do
			if msg.is_bot then
				local parsed = footer.deserialize(msg.content)
				if parsed then
					original_model = parsed.model
					original_width = original_width or parsed.width
					original_height = original_height or parsed.height
					original_denoise = original_denoise or parsed.denoise
					original_img2img = parsed.img2img or false
					break
				end
			end
		end
	end

	-- Require model
	if not original_model then
		error("Original model parameter not available - cannot regenerate image")
	end

	-- Default negative prompt
	original_negative = original_negative or paint.default_negative

	-- Get the new prompt from the latest user message
	local new_prompt = nil
	for i = #chain.messages, 1, -1 do
		if not chain.messages[i].is_bot then
			new_prompt = chain.messages[i].content
			break
		end
	end

	if not new_prompt or new_prompt == "" then
		error("Please provide a prompt in your reply.")
	end

	-- Only do img2img if the original was img2img
	local source_image_url = nil
	local denoise = nil
	if original_img2img then
		-- Find the last bot message with image attachments
		for i = #chain.messages, 1, -1 do
			local msg = chain.messages[i]
			if msg.is_bot and #msg.attachments > 0 then
				source_image_url = msg.attachments[1]
				break
			end
		end
		denoise = original_denoise or paint.default_denoise
	end

	-- Generate a new seed for variation
	local new_seed = math.random(1, 2147483647)

	-- Get model info from the original model
	local model_info = get_model_info(original_model)

	-- Generate the image with the new prompt
	generate_image(
		new_prompt,
		original_negative,
		new_seed,
		model_info,
		original_width,
		original_height,
		source_image_url,
		denoise
	)
end)
