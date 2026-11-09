//! Compiles the Slint GUI's `.slint` files when the `gui-slint` feature is on.

fn main() {
    #[cfg(feature = "gui-slint")]
    {
        // Debug info lets tests find elements by id with Slint's testing backend.
        let config = slint_build::CompilerConfiguration::new()
            .with_style("fluent".into())
            .with_debug_info(true);
        slint_build::compile_with_config("ui/app.slint", config)
            .expect("the Slint GUI's .slint files compile");
    }
}
