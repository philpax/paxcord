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

-- Build model choices for command options (uses global IMAGE_MODELS from main.lua)
local model_choices = map(IMAGE_MODELS, function(model)
	return {
		name = model.name .. " (" .. model.arch .. ")",
		value = model.name,
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

		local response = ask_llm {
			prompt = prompt,
			model = model,
			system = system,
			seed = seed,
		}

		output(response .. footer.serialize { model = model, seed = seed, system = system })
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
				.. IMAGE_DEFAULTS.denoise
				.. " for img2img, 1.0 for txt2img)",
			type = "number",
			required = false,
			min_value = 0.0,
			max_value = 1.0,
		},
	},
	execute = function(interaction)
		local result = generate_image {
			prompt = interaction.options.prompt,
			model = interaction.options.model,
			negative = interaction.options.negative,
			seed = interaction.options.seed,
			width = interaction.options.width,
			height = interaction.options.height,
			source_image_url = interaction.options.image or interaction.options.image_url,
			denoise = interaction.options.denoise,
		}

		attach("image_" .. result.seed .. ".png", result.image)
		output(footer.serialize {
			prompt = result.prompt,
			model = result.model,
			width = result.width,
			height = result.height,
			seed = result.seed,
			denoise = result.denoise,
			img2img = result.img2img,
		})
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

			llm.stream {
				messages = messages,
				model = model,
				seed = seed,
				callback = function(chunk)
					responses[#responses].response = chunk
					output(format_output())
					return true
				end,
			}
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
		local seed = interaction.options.seed or math.random(1, 2147483647)

		output("Generating prompt...")

		-- Generate a random prompt using Perchance
		local generator = "output = {import:prompt_generator}"
		local prompt = perchance.run(generator, seed)

		output("Generated prompt: " .. prompt)

		-- Generate the image using the generated prompt
		local result = generate_image {
			prompt = prompt,
			model = interaction.options.model,
			negative = interaction.options.negative,
			seed = seed,
			width = interaction.options.width,
			height = interaction.options.height,
		}

		attach("image_" .. result.seed .. ".png", result.image)
		output(footer.serialize {
			prompt = result.prompt,
			model = result.model,
			width = result.width,
			height = result.height,
			seed = result.seed,
			denoise = result.denoise,
			img2img = result.img2img,
		})
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

	-- Stream the response using ask_llm with pre-built messages
	local response = ask_llm {
		messages = messages,
		model = model,
		seed = original_seed,
	}

	output(response .. footer.serialize { model = model, seed = original_seed, system = system })
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
		local model = GPU_2_RESIDENT_MODEL

		local response = ask_llm {
			prompt = "Translate to " .. language .. ":\n\n" .. text,
			model = model,
			system = "You are a translator. Translate the user's text to the specified language. Output only the translation, nothing else.",
			seed = seed,
		}

		output(response .. footer.serialize { model = model, seed = seed, language = language })
	end,
}

-- System prompts for FPS prompt sanitization
local fpsprompt_system_hand = [[
Rewrite prompts for first-person image generation. Output ONLY the rewritten prompt in <=50 words, no commentary.

RULES:
- First-person view, 16:9, gloved hand holding an item in lower-right corner (partially cropped by frame edge)
- No gore, no brands/IP, no text in scene
- Sharp, stable framing, clear edges

EXAMPLES:
- "zombie apocalypse shotgun" -> "First-person view, abandoned city street, gloved hand gripping pump-action shotgun in lower-right corner partially cropped by frame edge, debris and overturned cars, overcast sky, sharp stable framing"
- "fantasy dungeon exploration" -> "First-person view, torch-lit stone dungeon corridor, gloved hand holding lantern in lower-right corner partially cropped by frame edge, ancient pillars, glowing runes on walls, dramatic lighting"
- "space station corridor" -> "First-person view, sleek white sci-fi corridor with blue accent lighting, gloved hand gripping scanner device in lower-right corner partially cropped by frame edge, windows showing stars, clean sharp edges"
]]

local fpsprompt_system_no_hand = [[
Rewrite prompts for first-person image generation. Output ONLY the rewritten prompt in <=50 words, no commentary.

RULES:
- First-person view, 16:9, no hands or body parts visible
- No gore, no brands/IP, no text in scene
- Sharp, stable framing, clear edges

EXAMPLES:
- "zombie apocalypse" -> "First-person view, abandoned city street, debris and overturned cars, distant shambling figures, overcast sky, sharp stable framing"
- "fantasy dungeon exploration" -> "First-person view, torch-lit stone dungeon corridor, ancient pillars, glowing runes on walls, dramatic lighting, sharp edges"
- "space station corridor" -> "First-person view, sleek white sci-fi corridor with blue accent lighting, windows showing stars, clean sharp edges, stable framing"
]]

local function get_fpsprompt_system(no_hand)
	if no_hand then
		return fpsprompt_system_no_hand
	end
	return fpsprompt_system_hand
end

-- Register the /perchancefpsprompt command
discord.register_command {
	name = "perchancefpsprompt",
	description = "Generate random FPS-style image prompts using Perchance + LLM sanitization",
	options = {
		{
			name = "count",
			description = "Number of prompts to generate (default: 5)",
			type = "integer",
			required = false,
			min_value = 1,
			max_value = 10,
		},
		{
			name = "no_hand",
			description = "Disable gloved hand holding item (default: false)",
			type = "boolean",
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
		local count = interaction.options.count or 5
		local no_hand = interaction.options.no_hand or false
		local seed = interaction.options.seed or math.random(1, 2147483647)
		local model = GPU_2_RESIDENT_MODEL
		local generator = "output = {import:prompt_generator}"
		local system = get_fpsprompt_system(no_hand)

		output("Generating " .. count .. " FPS prompts (seed: " .. seed .. ")...")

		local results = {}
		for i = 0, count - 1 do
			local raw_prompt = perchance.run(generator, seed + i)

			output("Generating " .. count .. " FPS prompts (seed: " .. seed .. ")...\n\nSanitizing prompt " .. (i + 1) .. "/" .. count .. "...")

			local sanitized = ask_llm {
				prompt = raw_prompt,
				model = model,
				system = system,
				seed = seed + i,
			}

			table.insert(results, (i + 1) .. ". " .. string.trim(sanitized))

			output("Generating " .. count .. " FPS prompts (seed: " .. seed .. ")...\n\n" .. table.concat(results, "\n\n"))
		end

		output(table.concat(results, "\n\n"))
	end,
}

-- Register the /transformfpsprompt command
discord.register_command {
	name = "transformfpsprompt",
	description = "Transform a prompt into FPS-style image generation format",
	options = {
		{
			name = "prompt",
			description = "The prompt to transform",
			type = "string",
			required = true,
		},
		{
			name = "no_hand",
			description = "Disable gloved hand holding item (default: false)",
			type = "boolean",
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
		local no_hand = interaction.options.no_hand or false
		local seed = interaction.options.seed or math.random(1, 2147483647)
		local model = GPU_2_RESIDENT_MODEL
		local system = get_fpsprompt_system(no_hand)

		local sanitized = ask_llm {
			prompt = prompt,
			model = model,
			system = system,
			seed = seed,
		}

		output(string.trim(sanitized) .. footer.serialize { seed = seed, no_hand = no_hand })
	end,
}

-- Register the /ocr command
discord.register_command {
	name = "ocr",
	description = "Extract text from an image using AI vision",
	options = {
		{
			name = "image",
			description = "Image to extract text from (upload)",
			type = "attachment",
			required = false,
		},
		{
			name = "image_url",
			description = "Image URL to extract text from",
			type = "string",
			required = false,
		},
	},
	execute = function(interaction)
		local image_url = interaction.options.image or interaction.options.image_url
		if not image_url then
			error("Please provide an image (upload or URL)")
		end

		local result = ocr { image_url = image_url }
		output(result)
	end,
}

-- Register the /describeimage command
discord.register_command {
	name = "describeimage",
	description = "Describe an image using AI vision",
	options = {
		{
			name = "image",
			description = "Image to describe (upload)",
			type = "attachment",
			required = false,
		},
		{
			name = "image_url",
			description = "Image URL to describe",
			type = "string",
			required = false,
		},
		{
			name = "prompt",
			description = "Custom prompt (default: 'Describe this image in detail.')",
			type = "string",
			required = false,
		},
	},
	execute = function(interaction)
		local image_url = interaction.options.image or interaction.options.image_url
		if not image_url then
			error("Please provide an image (upload or URL)")
		end

		local result = describe_image {
			image_url = image_url,
			prompt = interaction.options.prompt,
		}
		output(result)
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
	original_negative = original_negative or IMAGE_DEFAULTS.negative

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
		denoise = original_denoise or IMAGE_DEFAULTS.denoise
	end

	-- Generate a new seed for variation
	local new_seed = math.random(1, 2147483647)

	-- Generate the image with the new prompt
	local result = generate_image {
		prompt = new_prompt,
		model = original_model,
		negative = original_negative,
		seed = new_seed,
		width = original_width,
		height = original_height,
		source_image_url = source_image_url,
		denoise = denoise,
	}

	attach("image_" .. result.seed .. ".png", result.image)
	output(footer.serialize {
		prompt = result.prompt,
		model = result.model,
		width = result.width,
		height = result.height,
		seed = result.seed,
		denoise = result.denoise,
		img2img = result.img2img,
	})
end)
