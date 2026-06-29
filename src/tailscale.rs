use serde::Deserialize;
use std::process::Command;

#[derive(Deserialize, Debug, Clone)]
pub struct TailscaleStatus {
    #[serde(rename = "BackendState")]
    pub backend_state: String,
    #[serde(rename = "TailscaleIPs")]
    pub tailscale_ips: Option<Vec<String>>,
    #[serde(rename = "Self")]
    pub self_node: Option<TailscaleSelfNode>,
    #[serde(rename = "CurrentTailnet")]
    pub current_tailnet: Option<TailscaleTailnet>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TailscaleSelfNode {
    #[serde(rename = "HostName")]
    pub host_name: String,
    #[serde(rename = "Online")]
    pub online: bool,
    #[serde(rename = "TailscaleIPs")]
    pub tailscale_ips: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TailscaleTailnet {
    #[serde(rename = "Name")]
    pub name: String,
}

pub fn query_tailscale_status() -> Result<TailscaleStatus, String> {
    let output = Command::new("tailscale")
        .args(&["status", "--json"])
        .output()
        .map_err(|e| format!("Не вдалося запустити tailscale: {}. Перевірте, чи встановлено Tailscale.", e))?;

    if !output.status.success() {
        let err_str = String::from_utf8_lossy(&output.stderr).to_string();
        if err_str.contains("connect to local tailscaled") || err_str.contains("tailscaled") {
            return Err("Служба tailscaled не запущена.".to_string());
        }
        return Err(if err_str.trim().is_empty() {
            "Tailscale повернув помилку.".to_string()
        } else {
            err_str.trim().to_string()
        });
    }

    let status: TailscaleStatus = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("Помилка аналізу статусу Tailscale: {}", e))?;
        
    Ok(status)
}

pub fn start_tailscale_daemon() {
    let has_systemctl = Command::new("which")
        .arg("systemctl")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_systemctl {
        let _ = Command::new("pkexec")
            .args(&["systemctl", "start", "tailscaled"])
            .spawn();
    } else {
        let _ = Command::new("pkexec")
            .args(&["rc-service", "tailscaled", "start"])
            .spawn();
    }
}
