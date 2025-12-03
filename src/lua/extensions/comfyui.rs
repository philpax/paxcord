use std::sync::{Arc, OnceLock};

use mlua::{Lua, LuaSerdeExt, Result};
use parking_lot::Mutex;
use rucomfyui::object_info::ObjectInfo;
use rucomfyui::workflow::{Workflow, WorkflowGraph, WorkflowInput, WorkflowNode};

const COMFYUI_URL: &str = "http://127.0.0.1:8188";

/// Cached object info for lazy fetching
static OBJECT_INFO_CACHE: OnceLock<Mutex<Option<ObjectInfo>>> = OnceLock::new();

fn get_cache() -> &'static Mutex<Option<ObjectInfo>> {
    OBJECT_INFO_CACHE.get_or_init(|| Mutex::new(None))
}

async fn get_or_fetch_object_info() -> std::result::Result<ObjectInfo, rucomfyui::ClientError> {
    // Check cache first
    {
        let cache = get_cache().lock();
        if let Some(ref cached) = *cache {
            return Ok(cached.clone());
        }
    }

    // Fetch object info from the server
    let client = rucomfyui::Client::new(COMFYUI_URL);
    let object_info = client.get_object_info().await?;

    // Cache it
    {
        let mut cache = get_cache().lock();
        *cache = Some(object_info.clone());
    }

    Ok(object_info)
}

/// Build an SDXL workflow
fn build_sdxl_workflow(prompt: &str, negative_prompt: &str, seed: i64) -> Workflow {
    let graph = WorkflowGraph::new();

    // CheckpointLoaderSimple
    let checkpoint = graph.add_dynamic(WorkflowNode::new("CheckpointLoaderSimple").with_input(
        "ckpt_name",
        WorkflowInput::String("sd_xl_base_1.0.safetensors".to_string()),
    ));

    // Positive CLIP Text Encode
    let positive_clip = graph.add_dynamic(
        WorkflowNode::new("CLIPTextEncode")
            .with_input("text", WorkflowInput::String(prompt.to_string()))
            .with_input("clip", WorkflowInput::slot(checkpoint, 1)), // clip is output 1
    );

    // Negative CLIP Text Encode
    let negative_clip = graph.add_dynamic(
        WorkflowNode::new("CLIPTextEncode")
            .with_input("text", WorkflowInput::String(negative_prompt.to_string()))
            .with_input("clip", WorkflowInput::slot(checkpoint, 1)),
    );

    // Empty Latent Image
    let empty_latent = graph.add_dynamic(
        WorkflowNode::new("EmptyLatentImage")
            .with_input("width", WorkflowInput::I64(1024))
            .with_input("height", WorkflowInput::I64(1024))
            .with_input("batch_size", WorkflowInput::I64(1)),
    );

    // KSampler
    let ksampler = graph.add_dynamic(
        WorkflowNode::new("KSampler")
            .with_input("model", WorkflowInput::slot(checkpoint, 0)) // model is output 0
            .with_input("seed", WorkflowInput::I64(seed))
            .with_input("steps", WorkflowInput::I64(20))
            .with_input("cfg", WorkflowInput::F64(8.0))
            .with_input("sampler_name", WorkflowInput::String("euler".to_string()))
            .with_input("scheduler", WorkflowInput::String("normal".to_string()))
            .with_input("positive", WorkflowInput::slot(positive_clip, 0))
            .with_input("negative", WorkflowInput::slot(negative_clip, 0))
            .with_input("latent_image", WorkflowInput::slot(empty_latent, 0))
            .with_input("denoise", WorkflowInput::F64(1.0)),
    );

    // VAE Decode
    let vae_decode = graph.add_dynamic(
        WorkflowNode::new("VAEDecode")
            .with_input("samples", WorkflowInput::slot(ksampler, 0))
            .with_input("vae", WorkflowInput::slot(checkpoint, 2)), // vae is output 2
    );

    // Preview Image (or SaveImage for actual output)
    let _preview = graph.add_dynamic(
        WorkflowNode::new("PreviewImage").with_input("images", WorkflowInput::slot(vae_decode, 0)),
    );

    graph.into_workflow()
}

/// Result from painting - contains image bytes
pub struct PaintResult {
    pub images: Vec<Vec<u8>>,
}

impl mlua::UserData for PaintResult {
    fn add_fields<F: mlua::UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("image_count", |_, this| Ok(this.images.len()));
    }

    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get_image", |_, this, index: usize| {
            this.images
                .get(index.saturating_sub(1)) // Lua is 1-indexed
                .cloned()
                .ok_or_else(|| mlua::Error::runtime("Image index out of bounds"))
        });
    }
}

pub fn register(lua: &Lua) -> Result<()> {
    let comfy = lua.create_table()?;

    // comfy.object_info() - lazily fetches and caches object info
    comfy.set(
        "object_info",
        lua.create_async_function(|lua, ()| async move {
            let object_info = get_or_fetch_object_info()
                .await
                .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;

            // Convert to Lua value
            lua.to_value(&object_info)
        })?,
    )?;

    // comfy.paintsdxl(prompt, negative_prompt?, seed?) - generates an SDXL image
    comfy.set(
        "paintsdxl",
        lua.create_async_function(
            |lua, (prompt, negative_prompt, seed): (String, Option<String>, Option<i64>)| async move {
                let negative = negative_prompt.unwrap_or_else(|| "text, watermark, blurry".to_string());
                let seed = seed.unwrap_or_else(|| rand::random::<i64>().abs());

                // Ensure object_info is cached (validates connection)
                get_or_fetch_object_info()
                    .await
                    .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;

                // Build the workflow
                let workflow = build_sdxl_workflow(&prompt, &negative, seed);

                // Queue and wait for results
                let client = rucomfyui::Client::new(COMFYUI_URL);
                let results = client
                    .easy_queue(&workflow)
                    .await
                    .map_err(|e| mlua::Error::ExternalError(Arc::new(e)))?;

                // Collect all images from the results
                let mut images = Vec::new();
                for (_node_id, output) in results {
                    for image in output.images {
                        images.push(image);
                    }
                }

                lua.create_userdata(PaintResult { images })
            },
        )?,
    )?;

    lua.globals().set("comfy", comfy)?;

    Ok(())
}
