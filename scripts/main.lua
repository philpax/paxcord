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

-- Async helper to stream LLM responses
function stream_llm_response(messages, model, seed)
	local full_response = ""

	llm.stream({
		messages = messages,
		model = model,
		seed = seed,
		callback = function(chunk)
			output(chunk)
			full_response = chunk
			return true
		end,
	})

	return full_response
end

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
