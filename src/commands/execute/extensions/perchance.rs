use std::sync::Arc;

use perchance_interpreter::CompiledProgram;

pub fn register(lua: &mlua::Lua) -> mlua::Result<()> {
    let perchance = lua.create_table()?;

    // Simple one-shot evaluation
    perchance.set(
        "evaluate",
        lua.create_function(|_lua, (template, seed): (String, u64)| {
            perchance_interpreter::evaluate_with_seed(&template, seed)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))
        })?,
    )?;

    // Compile a template for reuse
    perchance.set(
        "compile",
        lua.create_function(|lua, template: String| {
            let compiled = perchance_interpreter::compile_template(&template)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;
            lua.create_any_userdata(compiled)
        })?,
    )?;

    lua.globals().set("perchance", perchance)?;

    Ok(())
}

impl mlua::UserData for CompiledProgram {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("evaluate", |_lua, this, seed: u64| {
            use rand::SeedableRng;
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            perchance_interpreter::evaluate(this, &mut rng)
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))
        });
    }
}
