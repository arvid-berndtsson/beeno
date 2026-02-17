use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::{Child, Command};

#[derive(Debug, Clone)]
pub struct ServerStatus {
    pub running: bool,
    pub port: u16,
    pub url: String,
    pub mode: String,
}

pub struct ServerManager {
    child: Option<Child>,
    source_path: Option<PathBuf>,
    source_code: Option<String>,
    port: Option<u16>,
    mode: Option<String>,
}

impl Default for ServerManager {
    fn default() -> Self {
        Self {
            child: None,
            source_path: None,
            source_code: None,
            port: None,
            mode: None,
        }
    }
}

impl ServerManager {
    pub async fn start_with_code(
        &mut self,
        code: String,
        port: u16,
        mode: &str,
    ) -> anyhow::Result<ServerStatus> {
        self.stop().await?;

        let source_path = temp_server_module_path();
        fs::write(&source_path, &code)?;

        let mut cmd = Command::new("deno");
        cmd.arg("run")
            .arg("--allow-net")
            .arg("--allow-read")
            .arg("--allow-env")
            .arg("--allow-write")
            .arg(&source_path)
            .env("PORT", format!("{port}"))
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdin(Stdio::null());

        let child = cmd.spawn()?;
        self.child = Some(child);
        self.source_path = Some(source_path);
        self.source_code = Some(code);
        self.port = Some(port);
        self.mode = Some(mode.to_string());

        Ok(self.status().unwrap_or(ServerStatus {
            running: true,
            port,
            url: format!("http://127.0.0.1:{port}"),
            mode: mode.to_string(),
        }))
    }

    pub async fn hotfix_with_code(
        &mut self,
        code: String,
        mode: &str,
    ) -> anyhow::Result<ServerStatus> {
        let port = self.port.unwrap_or(8080);
        self.start_with_code(code, port, mode).await
    }

    pub async fn stop(&mut self) -> anyhow::Result<()> {
        if let Some(child) = &mut self.child {
            let _ = child.start_kill();
            let _ = child.wait().await;
        }
        self.child = None;
        Ok(())
    }

    pub fn status(&mut self) -> Option<ServerStatus> {
        let child = self.child.as_mut()?;
        if let Ok(Some(_status)) = child.try_wait() {
            self.child = None;
            return None;
        }

        let port = self.port.unwrap_or(8080);
        Some(ServerStatus {
            running: true,
            port,
            url: format!("http://127.0.0.1:{port}"),
            mode: self.mode.clone().unwrap_or_else(|| "js".to_string()),
        })
    }

    pub fn last_source(&self) -> Option<String> {
        self.source_code.clone()
    }
}

fn temp_server_module_path() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("beeno-server-{millis}-{}.ts", std::process::id()))
}
