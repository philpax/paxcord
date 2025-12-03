use std::sync::Arc;

use perchance_interpreter::CompiledProgram;

pub fn register(lua: &mlua::Lua) -> mlua::Result<()> {
    let perchance = lua.create_table()?;

    let loader = Arc::new(
        perchance_interpreter::loader::ChainLoader::new()
            .with_loader(Arc::new(perchance_interpreter::loader::FolderLoader::new(
                "generators".into(),
            )))
            .with_loader(Arc::new(
                perchance_interpreter::loader::BuiltinGeneratorsLoader::new(),
            )),
    );

    // Simple one-shot evaluation
    perchance.set(
        "run",
        lua.create_async_function({
            let loader = loader.clone();
            move |_lua, (template, seed): (String, Option<u64>)| {
                let loader = loader.clone();
                async move {
                    perchance_interpreter::run(
                        &template,
                        perchance_interpreter::EvaluateOptions::new(rng_from_seed(seed))
                            .with_loader(loader),
                    )
                    .await
                    .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))
                }
            }
        })?,
    )?;

    // Compile a template for reuse
    perchance.set(
        "compile",
        lua.create_function({
            let loader = loader.clone();
            move |lua, template: String| {
                let compiled = perchance_interpreter::compile(
                    &perchance_interpreter::parse(&template)
                        .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?,
                )
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;
                lua.create_any_userdata(PerchanceProgram {
                    program: compiled,
                    loader: loader.clone(),
                })
            }
        })?,
    )?;

    lua.globals().set("perchance", perchance)?;

    Ok(())
}
// Newtype wrapper to satisfy the orphan rule
struct PerchanceProgram {
    program: CompiledProgram,
    loader: Arc<dyn perchance_interpreter::GeneratorLoader>,
}
impl mlua::UserData for PerchanceProgram {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("evaluate", |_lua, this, seed: Option<u64>| async move {
            let rng = rng_from_seed(seed);
            perchance_interpreter::evaluate(
                &this.program,
                perchance_interpreter::EvaluateOptions::new(rng).with_loader(this.loader.clone()),
            )
            .await
            .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))
        });
    }
}

fn rng_from_seed(seed: Option<u64>) -> rand::rngs::StdRng {
    use rand::SeedableRng;
    if let Some(seed) = seed {
        rand::rngs::StdRng::seed_from_u64(seed)
    } else {
        rand::rngs::StdRng::from_entropy()
    }
}
