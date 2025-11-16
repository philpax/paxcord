-- scripts/main.lua
-- Shared helpers and utilities available to both /execute and command handlers

-- Helper function to format output nicely
function format_output(...)
    local args = {...}
    local result = {}
    for i, v in ipairs(args) do
        if type(v) == "table" then
            table.insert(result, inspect(v))
        else
            table.insert(result, tostring(v))
        end
    end
    return table.concat(result, " ")
end

-- Helper to output formatted text
function out(...)
    return output(format_output(...))
end

-- Async helper to stream LLM responses
function stream_llm_response(messages, model, seed)
    model = model or "gpt-4o-mini"

    local stream = llm.stream({
        messages = messages,
        model = model,
        seed = seed
    })

    local full_response = ""
    for chunk in stream do
        if chunk.content then
            full_response = full_response .. chunk.content
            output(full_response)
        end
    end

    return full_response
end

print("Main helpers loaded")
