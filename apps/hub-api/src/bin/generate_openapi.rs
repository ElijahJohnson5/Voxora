use utoipa::OpenApi;

fn main() {
    let spec = hub_api::routes::ApiDoc::openapi().to_pretty_json().unwrap();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs/hub-api.json");
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&out, spec).unwrap();
    println!("Wrote {}", out.display());
}
