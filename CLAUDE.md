# Paxcord

A personal Discord bot: a thin Rust wrapper around a Lua (Luau) scripting engine with AI and Discord functionality bound.

## Commands

- `cargo clippy` - Run linter (always use before committing)
- `cargo fmt` - Format code (always use before committing)
- **Never run `cargo build` or `cargo run`** - The bot may already be running

## Architecture

Commands are defined in Lua (`scripts/commands.lua`) and registered dynamically at startup. The Rust side provides:

- **Discord integration** (serenity): Event handling, slash command registration, message management
- **Lua extensions** (`src/lua/extensions/`): Bindings for LLM streaming, ComfyUI image generation, currency conversion, and output handling
- **Outputter** (`src/outputter.rs`): Manages Discord message updates with throttling via async channels (flume)
- **Reply chains** (`src/reply_handler.rs`): Multi-turn conversation support through message replies

Key files:
- `scripts/commands.lua` - All bot commands
- `src/lua/executor.rs` - Lua thread execution
- `src/commands/lua_command.rs` - Bridge between Discord interactions and Lua
