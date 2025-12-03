-- scripts/main.lua
-- Shared helpers and utilities available to both /execute and command handlers

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
