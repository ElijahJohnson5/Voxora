use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;

fn prompt(label: &str, default: Option<&str>) -> String {
    match default {
        Some(d) => print!("{} [{}]: ", label, d),
        None => print!("{}: ", label),
    }
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_string();
    if input.is_empty() {
        default.unwrap_or("").to_string()
    } else {
        input
    }
}

fn generate_code_verifier() -> String {
    let bytes: Vec<u8> = (0..32).map(|_| rand::thread_rng().gen()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn compute_code_challenge(verifier: &str) -> String {
    let hash = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hash)
}

fn read_env_file(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                map.insert(key.trim().to_string(), value.trim().to_string());
            }
        }
    }
    map
}

fn write_env_file(path: &Path, updates: &HashMap<String, String>) {
    let mut lines: Vec<String> = Vec::new();
    let mut written_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some((key, _)) = trimmed.split_once('=') {
                let key = key.trim();
                if updates.contains_key(key) {
                    lines.push(format!("{}={}", key, updates[key]));
                    written_keys.insert(key.to_string());
                    continue;
                }
            }
            lines.push(line.to_string());
        }
    }

    for (key, value) in updates {
        if !written_keys.contains(key) {
            lines.push(format!("{}={}", key, value));
        }
    }

    // Ensure trailing newline
    let mut content = lines.join("\n");
    if !content.ends_with('\n') {
        content.push('\n');
    }

    std::fs::write(path, content).expect("Failed to write .env file");
}

fn main() {
    println!("=== Voxora Pod Setup ===\n");

    let env_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
    let env_path = env_path.as_path();
    let env_vars = read_env_file(env_path);

    let default_hub_url = env_vars
        .get("HUB_URL")
        .cloned()
        .unwrap_or_else(|| "http://localhost:4001".to_string());
    let default_port = env_vars
        .get("PORT")
        .cloned()
        .unwrap_or_else(|| "4002".to_string());

    let hub_url = prompt("Hub URL", Some(&default_hub_url));
    let username = prompt("Hub username", None);
    print!("Hub password: ");
    io::stdout().flush().unwrap();
    let password = rpassword::read_password().expect("Failed to read password");
    let pod_name = prompt("Pod name", Some("Dev Pod"));
    let default_pod_url = format!("http://localhost:{}", default_port);
    let pod_url = prompt("Pod URL", Some(&default_pod_url));

    println!("\nAuthenticating with Hub...");

    // PKCE
    let code_verifier = generate_code_verifier();
    let code_challenge = compute_code_challenge(&code_verifier);

    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("Failed to build HTTP client");

    // Step 1: POST /oidc/authorize
    let authorize_url = format!("{}/oidc/authorize", hub_url);
    let authorize_resp = client
        .post(&authorize_url)
        .form(&[
            ("response_type", "code"),
            ("client_id", "voxora-web"),
            ("redirect_uri", "http://localhost:0/callback"),
            ("scope", "openid profile email pods"),
            ("code_challenge", &code_challenge),
            ("code_challenge_method", "S256"),
            ("login", &username),
            ("password", &password),
        ])
        .send()
        .expect("Failed to connect to Hub. Is it running?");

    if !authorize_resp.status().is_redirection() {
        let status = authorize_resp.status();
        let body = authorize_resp.text().unwrap_or_default();
        eprintln!("Authorization failed (HTTP {}): {}", status, body);
        std::process::exit(1);
    }

    let location = authorize_resp
        .headers()
        .get("location")
        .expect("No Location header in redirect")
        .to_str()
        .expect("Invalid Location header");

    let redirect_url = reqwest::Url::parse(location).expect("Failed to parse redirect URL");
    let code = redirect_url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .expect("No authorization code in redirect");

    println!("Authorization code obtained.");

    // Step 2: POST /oidc/token
    let token_url = format!("{}/oidc/token", hub_url);
    let token_resp = client
        .post(&token_url)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", "http://localhost:0/callback"),
            ("code_verifier", &code_verifier),
            ("client_id", "voxora-web"),
        ])
        .send()
        .expect("Failed to exchange authorization code");

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let body = token_resp.text().unwrap_or_default();
        eprintln!("Token exchange failed (HTTP {}): {}", status, body);
        std::process::exit(1);
    }

    let token_data: serde_json::Value = token_resp.json().expect("Invalid token response JSON");
    let access_token = token_data["access_token"]
        .as_str()
        .expect("No access_token in response");

    println!("Access token obtained.");

    // Step 3: POST /api/v1/pods/register
    println!("Registering pod...");
    let register_url = format!("{}/api/v1/pods/register", hub_url);
    let register_resp = client
        .post(&register_url)
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "name": pod_name,
            "url": pod_url,
        }))
        .send()
        .expect("Failed to register pod");

    if !register_resp.status().is_success() {
        let status = register_resp.status();
        let body = register_resp.text().unwrap_or_default();
        eprintln!("Pod registration failed (HTTP {}): {}", status, body);
        std::process::exit(1);
    }

    let reg_data: serde_json::Value = register_resp.json().expect("Invalid registration response");
    let pod_id = reg_data["pod_id"].as_str().expect("No pod_id in response");
    let client_id = reg_data["client_id"]
        .as_str()
        .expect("No client_id in response");
    let client_secret = reg_data["client_secret"]
        .as_str()
        .expect("No client_secret in response");

    // Step 4: Update .env
    let mut updates = HashMap::new();
    updates.insert("POD_ID".to_string(), pod_id.to_string());
    updates.insert("POD_CLIENT_ID".to_string(), client_id.to_string());
    updates.insert("POD_CLIENT_SECRET".to_string(), client_secret.to_string());
    write_env_file(env_path, &updates);

    println!("\n=== Pod registered successfully! ===");
    println!("  POD_ID:            {}", pod_id);
    println!("  POD_CLIENT_ID:     {}", client_id);
    println!("  POD_CLIENT_SECRET: {}", client_secret);
    println!("\nCredentials written to .env");
    println!("Start your pod with: cargo run -p pod-api");
}
