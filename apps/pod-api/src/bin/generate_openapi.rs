use utoipa::OpenApi;

fn main() {
    let spec = pod_api::routes::ApiDoc::openapi().to_pretty_json().unwrap();
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs/pod-api.json");
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&out, spec).unwrap();
    println!("Wrote {}", out.display());
}
