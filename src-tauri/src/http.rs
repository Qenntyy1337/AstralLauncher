use std::sync::OnceLock;

const USER_AGENT: &str = "AstralLauncher/1.0.0 (github.com/astrallauncher; modrinth integration)";

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(20))
            .pool_max_idle_per_host(8)
            .build()
            .expect("failed to build http client")
    })
}

pub async fn get_json(url: &str) -> Result<serde_json::Value, String> {
    let resp = client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("GET {}: {}", url, e))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("HTTP {} for {}", status, url));
    }
    serde_json::from_str(&text).map_err(|e| format!("bad json from {}: {}", url, e))
}
