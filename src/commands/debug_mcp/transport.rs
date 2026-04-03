use anyhow::Context;
use anyhow::anyhow;
use rmcp::RoleServer;
use rmcp::model::ClientNotification;
use rmcp::model::ClientRequest;
use rmcp::model::InitializedNotification;
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use serde::Serialize;
use serde_json::json;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::io::BufWriter;

pub(super) struct ContentLengthStdioTransport {
    input: BufReader<tokio::io::Stdin>,
    output: std::sync::Arc<tokio::sync::Mutex<BufWriter<tokio::io::Stdout>>>,
    pending_initialized_notification: bool,
}

impl ContentLengthStdioTransport {
    pub(super) fn new() -> Self {
        Self {
            input: BufReader::new(tokio::io::stdin()),
            output: std::sync::Arc::new(tokio::sync::Mutex::new(BufWriter::new(
                tokio::io::stdout(),
            ))),
            pending_initialized_notification: false,
        }
    }

    async fn read_message(&mut self) -> anyhow::Result<Option<RxJsonRpcMessage<RoleServer>>> {
        let mut content_length = None;

        loop {
            let mut line = String::new();
            let read_size = self.input.read_line(&mut line).await?;
            if read_size == 0 {
                return Ok(None);
            }

            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                if content_length.is_some() {
                    break;
                }
                continue;
            }

            let Some((header, value)) = trimmed.split_once(':') else {
                return Err(anyhow!("Invalid header: {trimmed}"));
            };

            if header.trim().eq_ignore_ascii_case("Content-Length") {
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .with_context(|| format!("Invalid Content-Length header: {trimmed}"))?,
                );
            }
        }

        let Some(content_length) = content_length else {
            return Err(anyhow!("Missing Content-Length header"));
        };

        let mut payload = vec![0_u8; content_length];
        self.input.read_exact(&mut payload).await?;
        serde_json::from_slice(&payload).context("Failed to decode JSON-RPC payload")
    }

    async fn write_message<T: Serialize>(&self, payload: &T) -> std::io::Result<()> {
        let encoded = serde_json::to_vec(payload)?;
        let mut output = self.output.lock().await;
        output
            .write_all(format!("Content-Length: {}\r\n\r\n", encoded.len()).as_bytes())
            .await?;
        output.write_all(&encoded).await?;
        output.flush().await
    }

    async fn write_parse_error(&self, message: impl Into<String>) {
        let payload = json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32700,
                "message": message.into(),
            }
        });
        let _ = self.write_message(&payload).await;
    }
}

impl Transport<RoleServer> for ContentLengthStdioTransport {
    type Error = std::io::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let output = self.output.clone();
        async move {
            let encoded = serde_json::to_vec(&item)?;
            let mut output = output.lock().await;
            output
                .write_all(format!("Content-Length: {}\r\n\r\n", encoded.len()).as_bytes())
                .await?;
            output.write_all(&encoded).await?;
            output.flush().await
        }
    }

    fn receive(&mut self) -> impl Future<Output = Option<RxJsonRpcMessage<RoleServer>>> + Send {
        async move {
            if self.pending_initialized_notification {
                self.pending_initialized_notification = false;
                return Some(<RxJsonRpcMessage<RoleServer>>::notification(
                    ClientNotification::InitializedNotification(InitializedNotification {
                        method: Default::default(),
                        extensions: Default::default(),
                    }),
                ));
            }

            loop {
                match self.read_message().await {
                    Ok(Some(message)) => {
                        let is_initialize_request =
                            if let rmcp::model::JsonRpcMessage::Request(request) = &message {
                                matches!(request.request, ClientRequest::InitializeRequest(_))
                            } else {
                                false
                            };
                        if is_initialize_request {
                            self.pending_initialized_notification = true;
                        }
                        return Some(message);
                    }
                    Ok(None) => return None,
                    Err(err) => {
                        self.write_parse_error(err.to_string()).await;
                    }
                }
            }
        }
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        let mut output = self.output.lock().await;
        output.flush().await
    }
}
