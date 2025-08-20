fn main() {
    // Ensure proper linking against Python on macOS and pick correct config
    pyo3_build_config::add_extension_module_link_args();
}
