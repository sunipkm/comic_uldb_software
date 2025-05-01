#![deny(missing_docs)]
//!   
//! # Generic Camera GUI
//! This is the entry point when compiled to WebAssembly.
//!   

use eframe::wasm_bindgen::{self, prelude::*};

/// This is the entry-point for all the web-assembly.
/// This is called once from the HTML.
/// It loads the app, installs some callbacks, then returns.
/// You can add more callbacks like this if you want to call in to your code.
#[wasm_bindgen]
pub async fn start(canvas: web_sys::HtmlCanvasElement) -> Result<(), JsValue> {
    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let app = crate::GenCamGUI::default();
    eframe::WebRunner::new()
        .start(
            "gui_canvas",
            Default::default(),
            // Box::new(|_cc| Ok(Box::new(app))),
            Box::new(|cc| {
                // This gives us image support:
                egui_extras::install_image_loaders(&cc.egui_ctx);
                Ok(Box::new(app))
            }),
        )
        .await?;
    Ok(())
}
