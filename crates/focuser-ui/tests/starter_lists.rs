//! The starter category lists exist twice on purpose.
//!
//! The desktop frontend fetches its copy at runtime; the extension bundles its
//! own so `extension/` builds without the rest of the repo. AMO requires a
//! source archive a reviewer can build, and reaching across to `../crates/`
//! made that impossible.
//!
//! Two copies drift unless something says so. This is that something.

use std::path::PathBuf;

fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn starter_lists_match() {
    let app = repo_file("crates/focuser-ui/frontend/public/premade-lists.json");
    let extension = repo_file("extension/public/premade-lists.json");

    let app_json = std::fs::read_to_string(&app).expect("the app's starter lists are missing");
    let extension_json =
        std::fs::read_to_string(&extension).expect("the extension's starter lists are missing");

    // Parsed rather than compared byte for byte, so a line ending or a trailing
    // newline is not a failing test.
    let app_value: serde_json::Value =
        serde_json::from_str(&app_json).expect("app copy is not JSON");
    let extension_value: serde_json::Value =
        serde_json::from_str(&extension_json).expect("extension copy is not JSON");

    assert_eq!(
        app_value, extension_value,
        "premade-lists.json has drifted. Copy the app's version to extension/public/."
    );
}
