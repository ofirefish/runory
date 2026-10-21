pub mod compatibility;
pub mod logo;
pub mod model;
pub mod parser;
pub mod validator;
pub mod version;

pub use compatibility::validate_compatibility;
pub use logo::validate_logo_file;
pub use model::PluginManifest;
pub use parser::parse_manifest_json;
pub use validator::validate_manifest;
