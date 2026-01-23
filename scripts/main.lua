-- scripts/main.lua
-- Shared helpers and utilities available to both /execute and command handlers

-- String helpers
function string.trim(s)
	return s:match("^%s*(.-)%s*$")
end

function string.split(s, delimiter)
	local result = {}
	local pattern = string.format("([^%s]+)", delimiter)
	for match in s:gmatch(pattern) do
		table.insert(result, match)
	end
	return result
end

function string.starts_with(s, prefix)
	return s:sub(1, #prefix) == prefix
end

function string.ends_with(s, suffix)
	return s:sub(-#suffix) == suffix
end

-- Table helpers
function table.map(tbl, fn)
	local result = {}
	for i, v in ipairs(tbl) do
		result[i] = fn(v, i)
	end
	return result
end

function table.filter(tbl, predicate)
	local result = {}
	for i, v in ipairs(tbl) do
		if predicate(v, i) then
			table.insert(result, v)
		end
	end
	return result
end

function table.reduce(tbl, fn, initial)
	local acc = initial
	for i, v in ipairs(tbl) do
		acc = fn(acc, v, i)
	end
	return acc
end

function table.find(tbl, predicate)
	for i, v in ipairs(tbl) do
		if predicate(v, i) then
			return v, i
		end
	end
	return nil, nil
end

function table.includes(tbl, value)
	for _, v in ipairs(tbl) do
		if v == value then
			return true
		end
	end
	return false
end

function table.keys(tbl)
	local result = {}
	for k, _ in pairs(tbl) do
		table.insert(result, k)
	end
	return result
end

function table.values(tbl)
	local result = {}
	for _, v in pairs(tbl) do
		table.insert(result, v)
	end
	return result
end

function table.merge(...)
	local result = {}
	for _, tbl in ipairs({ ... }) do
		for k, v in pairs(tbl) do
			result[k] = v
		end
	end
	return result
end

function table.shallow_copy(tbl)
	local result = {}
	for k, v in pairs(tbl) do
		result[k] = v
	end
	return result
end

function table.reverse(tbl)
	local result = {}
	for i = #tbl, 1, -1 do
		table.insert(result, tbl[i])
	end
	return result
end

-- Global aliases for convenience
map = table.map
filter = table.filter
reduce = table.reduce

-- LLM model that stays resident on GPU 2
GPU_2_RESIDENT_MODEL = "gpu:qwen3-vl-30b-a3b-instruct"

-- ComfyUI lazy loading helpers
local comfy_client = nil
local comfy_object_info = nil

function get_comfy_client()
	if not comfy_client then
		comfy_client = comfy.client("http://127.0.0.1:8188")
	end
	return comfy_client
end

function get_comfy_object_info()
	if not comfy_object_info then
		local client = get_comfy_client()
		comfy_object_info = client:get_object_info()
	end
	return comfy_object_info
end

-- ============================================================================
-- Image generation model data
-- ============================================================================

-- Model definitions: { name, arch, checkpoint, keyword (optional) }
IMAGE_MODELS = {
	{ name = "ACertainModel", arch = "SD1", checkpoint = "ACertainModel ^SD1.ckpt" },
	{ name = "ACertainThing", arch = "SD1", checkpoint = "ACertainThing ^SD1.ckpt" },
	{ name = "AbyssOrangeMix2", arch = "SD1", checkpoint = "AbyssOrangeMix2_sfw ^SD1.safetensors" },
	{ name = "Analog", arch = "SD1", checkpoint = "Analog [analog style] ^SD1.safetensors", keyword = "analog style" },
	{ name = "Anything v3", arch = "SD1", checkpoint = "Anything v3.0 ^SD1.ckpt" },
	{ name = "Cinematic Diffusion", arch = "SD1", checkpoint = "Cinematic Diffusion [syberart] ^SD1.ckpt", keyword = "syberart" },
	{ name = "Dreamlike Photoreal", arch = "SD1", checkpoint = "Dreamlike Photoreal 2.0 ^SD1.safetensors" },
	{ name = "Dreamlike", arch = "SD1", checkpoint = "Dreamlike [dreamlike art] ^SD1.ckpt", keyword = "dreamlike art" },
	{ name = "Holosomnia Landscape", arch = "SD1", checkpoint = "Holosomnia Landscape [holosomnialandscape] ^SD1.ckpt", keyword = "holosomnialandscape" },
	{ name = "Inkpunk", arch = "SD1", checkpoint = "Inkpunk v2 [nvinkpunk] ^SD1.ckpt", keyword = "nvinkpunk" },
	{ name = "Pastel Mix", arch = "SD1", checkpoint = "Pastel Mix ^SD1.safetensors" },
	{ name = "Stable Diffusion 1.5", arch = "SD1", checkpoint = "Stable v1.5 ^SD1.ckpt" },
	{ name = "Van Gogh", arch = "SD1", checkpoint = "Van Gogh v2 [lvngvncnt] ^SD1.ckpt", keyword = "lvngvncnt" },
	{ name = "Vintedois", arch = "SD1", checkpoint = "Vintedois v0.1 [estilovintedois] ^SD1.ckpt", keyword = "estilovintedois" },
	{ name = "seek.art MEGA", arch = "SD1", checkpoint = "seek.art MEGA ^SD1.ckpt" },
	{ name = "Stable Diffusion 2.1", arch = "SD2", checkpoint = "Stable v2.1 ^SD2.ckpt" },
	{ name = "SDXL", arch = "SDXL", checkpoint = "SDXL 1.0 ^SDXL.safetensors" },
	{ name = "SDXL Turbo", arch = "SDXL", checkpoint = "SDXL Turbo 1.0 ^SDXL.safetensors" },
	{ name = "Nova Orange XL", arch = "SDXL", checkpoint = "Nova Orange XL v13.0 ^SDXL.safetensors" },
	{ name = "Nova Anime XL", arch = "SDXL", checkpoint = "Nova Anime XL IL v14.0 ^SDXL.safetensors" },
	{ name = "Illustrious", arch = "SDXL", checkpoint = "Illustrious v2.0 ^SDXL.safetensors" },
	{ name = "Z Image Turbo", arch = "ZImage" },
}

-- Build lookup table by name
IMAGE_MODEL_BY_NAME = {}
for _, model in ipairs(IMAGE_MODELS) do
	IMAGE_MODEL_BY_NAME[model.name] = model
end

-- Default dimensions per architecture
IMAGE_ARCH_DEFAULTS = {
	SD1 = { width = 512, height = 512 },
	SD2 = { width = 768, height = 768 },
	SDXL = { width = 1024, height = 1024 },
	ZImage = { width = 1024, height = 1024 },
}

-- Default values for image generation
IMAGE_DEFAULTS = {
	negative = "text, watermark, blurry",
	denoise = 0.8,
}

-- Get model by name
function get_image_model(name)
	local model = IMAGE_MODEL_BY_NAME[name]
	if not model then
		error("Unknown image model: " .. tostring(name))
	end
	return model
end

-- Get a random model
function get_random_image_model()
	local idx = math.random(1, #IMAGE_MODELS)
	return IMAGE_MODELS[idx]
end

-- ============================================================================
-- Global API Functions
-- All functions take a table of options for easy calling and clear error messages
-- ============================================================================

--- Perform OCR on an image using AI vision
--- @param opts table Options table
---   - image_url: string (optional) URL of the image to process
---   - image_data: string (optional) Raw image data (binary)
---   - model: string (optional) Vision model to use (default: "gpu:qwen3-vl-30b-a3b-instruct")
---   - seed: number (optional) Random seed
---   - output: function (optional) Output callback (default: output)
--- @return string The extracted text
function ocr(opts)
	if type(opts) ~= "table" then
		error("ocr() requires a table argument, e.g. ocr({image_url = '...'})")
	end

	local image_data = opts.image_data
	local image_url = opts.image_url

	if not image_data and not image_url then
		error("ocr() requires 'image_url' or 'image_data' in options")
	end

	local out = opts.output or output
	local model = opts.model or GPU_2_RESIDENT_MODEL
	local seed = opts.seed or math.random(1, 2147483647)

	-- Fetch image if URL provided
	if image_url and not image_data then
		out("Downloading image...")
		image_data = fetch(image_url)
	end

	out("Extracting text...")
	local messages = {
		llm.user {
			{
				type = "text",
				text = "Perform OCR on this image. Transcribe all text visible in this image. Output only the transcribed text with no additional commentary.",
			},
			{ type = "image", data = image_data },
		},
	}

	local full_response = ""
	llm.stream {
		messages = messages,
		model = model,
		seed = seed,
		callback = function(chunk)
			out(chunk)
			full_response = chunk
			return true
		end,
	}

	return string.trim(full_response)
end

--- Describe an image using AI vision
--- @param opts table Options table
---   - image_url: string (optional) URL of the image to describe
---   - image_data: string (optional) Raw image data (binary)
---   - prompt: string (optional) Custom prompt (default: "Describe this image in detail.")
---   - model: string (optional) Vision model to use (default: "gpu:qwen3-vl-30b-a3b-instruct")
---   - seed: number (optional) Random seed
---   - output: function (optional) Output callback (default: output)
--- @return string The description
function describe_image(opts)
	if type(opts) ~= "table" then
		error("describe_image() requires a table argument, e.g. describe_image({image_url = '...'})")
	end

	local image_data = opts.image_data
	local image_url = opts.image_url

	if not image_data and not image_url then
		error("describe_image() requires 'image_url' or 'image_data' in options")
	end

	local out = opts.output or output
	local prompt = opts.prompt or "Describe this image in detail."
	local model = opts.model or GPU_2_RESIDENT_MODEL
	local seed = opts.seed or math.random(1, 2147483647)

	-- Fetch image if URL provided
	if image_url and not image_data then
		out("Downloading image...")
		image_data = fetch(image_url)
	end

	out("Analyzing image...")
	local messages = {
		llm.user {
			{ type = "text", text = prompt },
			{ type = "image", data = image_data },
		},
	}

	local full_response = ""
	llm.stream {
		messages = messages,
		model = model,
		seed = seed,
		callback = function(chunk)
			out(chunk)
			full_response = chunk
			return true
		end,
	}

	return string.trim(full_response)
end

--- Ask an LLM a question and stream the response
--- @param opts table Options table
---   - prompt: string (required) The prompt to send
---   - model: string (required) The model to use
---   - system: string (optional) System prompt (default: "You are a helpful assistant.")
---   - seed: number (optional) Random seed
---   - messages: table (optional) Full message history (overrides prompt/system if provided)
---   - output: function (optional) Output callback (default: output)
--- @return string The response text
function ask_llm(opts)
	if type(opts) ~= "table" then
		error("ask_llm() requires a table argument, e.g. ask_llm({prompt = '...', model = '...'})")
	end

	local model = opts.model
	if not model then
		error("ask_llm() requires 'model' in options")
	end

	local messages = opts.messages
	if not messages then
		local prompt = opts.prompt
		if not prompt then
			error("ask_llm() requires 'prompt' or 'messages' in options")
		end

		local system = opts.system or "You are a helpful assistant."
		messages = {
			llm.system(system),
			llm.user(prompt),
		}
	end

	local out = opts.output or output
	local seed = opts.seed or math.random(1, 2147483647)

	out("Generating...")

	local full_response = ""
	llm.stream {
		messages = messages,
		model = model,
		seed = seed,
		callback = function(chunk)
			out(chunk)
			full_response = chunk
			return true
		end,
	}

	return string.trim(full_response)
end

--- Generate an image using ComfyUI
--- @param opts table Options table
---   - prompt: string (required) The image generation prompt
---   - model: string (optional) Model name from IMAGE_MODELS (default: random)
---   - negative: string (optional) Negative prompt (default: "text, watermark, blurry")
---   - seed: number (optional) Random seed
---   - width: number (optional) Image width (default: based on model architecture)
---   - height: number (optional) Image height (default: based on model architecture)
---   - source_image_url: string (optional) Source image URL for img2img
---   - source_image_data: string (optional) Source image data for img2img
---   - denoise: number (optional) Denoising strength for img2img (default: 0.8)
---   - output: function (optional) Output callback (default: output)
--- @return table { image, images, prompt, model, width, height, seed, denoise, img2img }
function generate_image(opts)
	if type(opts) ~= "table" then
		error("generate_image() requires a table argument, e.g. generate_image({prompt = '...'})")
	end

	local prompt = opts.prompt
	if not prompt then
		error("generate_image() requires 'prompt' in options")
	end

	local out = opts.output or output

	-- Get model by name or random
	local model_def
	if opts.model then
		model_def = get_image_model(opts.model)
	else
		model_def = get_random_image_model()
	end

	local negative = opts.negative or IMAGE_DEFAULTS.negative
	local seed = opts.seed or math.random(1, 2147483647)
	local width = opts.width
	local height = opts.height

	-- Handle source image for img2img
	local source_image_data = opts.source_image_data
	local source_image_url = opts.source_image_url
	local denoise = opts.denoise

	-- Apply prompt keyword if model has one
	local final_prompt = prompt
	if model_def.keyword then
		final_prompt = prompt .. ", " .. model_def.keyword
	end

	-- Use architecture defaults if dimensions not specified
	local defaults = IMAGE_ARCH_DEFAULTS[model_def.arch]
	width = width or defaults.width
	height = height or defaults.height

	out("Connecting to ComfyUI...")

	-- Get client and object info (lazily cached)
	local client = get_comfy_client()
	local object_info = get_comfy_object_info()

	out("Building workflow...")

	-- Create the graph
	local g = comfy.graph(object_info)

	-- Load model, clip, and vae based on architecture
	local model, clip, vae
	if model_def.arch == "ZImage" then
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
		local c = g:CheckpointLoaderSimple(model_def.checkpoint)
		model, clip, vae = c.model, c.clip, c.vae
	end

	-- Create latent image (swap node based on source image presence)
	local latent_image
	local has_source_image = source_image_data or source_image_url
	if has_source_image then
		-- img2img: fetch image, upload to ComfyUI, load, scale, encode
		if source_image_url and not source_image_data then
			out("Downloading source image...")
			source_image_data = fetch(source_image_url)
		end
		local uploaded = client:upload_image("source.png", source_image_data)
		local loaded = g:LoadImage { image = uploaded.name }
		local scaled = g:ImageScale {
			image = loaded.image,
			width = width,
			height = height,
			upscale_method = "bicubic",
			crop = "disabled",
		}
		latent_image = g:VAEEncode { pixels = scaled, vae = vae }
		denoise = denoise or IMAGE_DEFAULTS.denoise
	else
		-- txt2img: empty latent
		if model_def.arch == "ZImage" then
			latent_image = g:EmptySD3LatentImage { width = width, height = height, batch_size = 1 }
		else
			latent_image = g:EmptyLatentImage { width = width, height = height, batch_size = 1 }
		end
		denoise = 1.0
	end

	-- KSampler settings based on architecture
	local steps, cfg, scheduler
	if model_def.arch == "ZImage" then
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

	out("Generating image (seed: " .. seed .. ")...")

	-- Queue the workflow and wait for results
	local result = client:easy_queue(g)

	-- Get the images from the preview node
	local images = result[preview].images

	if #images == 0 then
		error("No images were generated.")
	end

	-- Return result info and image data
	return {
		image = images[1],
		images = images,
		prompt = prompt,
		model = model_def.name,
		width = width,
		height = height,
		seed = seed,
		denoise = denoise,
		img2img = has_source_image ~= nil,
	}
end