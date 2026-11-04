// SPDX-License-Identifier: GPL-2.0-or-later
//! Local control protocol: one JSON object per line over a Unix socket (prototype; see ADR-0008).
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    Peers,
    Dial { addr: String },
    Say { text: String },
    Inbox,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default)]
    pub data: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(data: serde_json::Value) -> Self {
        Response { ok: true, data, error: None }
    }
    pub fn err(message: impl Into<String>) -> Self {
        Response { ok: false, data: serde_json::Value::Null, error: Some(message.into()) }
    }
}

/// Send one request to the daemon listening on `socket` and wait for its answer.
pub async fn call(socket: &Path, req: &Request) -> Result<Response> {
    let stream = UnixStream::connect(socket)
        .await
        .with_context(|| format!("cannot reach luxd at {} (is it running?)", socket.display()))?;
    let (rd, mut wr) = stream.into_split();
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    wr.write_all(line.as_bytes()).await?;
    let mut lines = BufReader::new(rd).lines();
    match lines.next_line().await? {
        Some(l) => Ok(serde_json::from_str(&l).context("invalid answer from luxd")?),
        None => bail!("luxd closed the connection without answering"),
    }
}
