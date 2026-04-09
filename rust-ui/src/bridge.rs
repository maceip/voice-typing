use futures_util::{SinkExt, StreamExt};
use std::sync::{Mutex, OnceLock};
use tokio::sync::{broadcast, mpsc, watch};

const PORT: u16 = 36199;

#[derive(Debug, Clone)]
pub enum ExtensionCommand {
    ToggleListening,
}

pub struct GuiBridge {
    state_tx: watch::Sender<String>,
    outgoing_tx: broadcast::Sender<String>,
    command_rx: Mutex<mpsc::UnboundedReceiver<ExtensionCommand>>,
}

static BRIDGE: OnceLock<GuiBridge> = OnceLock::new();

pub fn get() -> Option<&'static GuiBridge> {
    BRIDGE.get()
}

impl GuiBridge {
    pub fn set_state(&self, state: &str) {
        let _ = self.state_tx.send(state.to_string());
        let json = serde_json::json!({"type": "status", "state": state});
        let _ = self.outgoing_tx.send(json.to_string());
    }

    pub fn send_transcript(&self, text: &str, is_final: bool) {
        let json = serde_json::json!({
            "type": "transcript",
            "text": text,
            "is_final": is_final
        });
        let _ = self.outgoing_tx.send(json.to_string());
    }

    pub fn try_recv_command(&self) -> Option<ExtensionCommand> {
        self.command_rx.lock().ok()?.try_recv().ok()
    }
}

pub fn spawn() {
    let (state_tx, state_rx) = watch::channel("idle".to_string());
    let (outgoing_tx, _) = broadcast::channel::<String>(256);
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    let _ = BRIDGE.set(GuiBridge {
        state_tx,
        outgoing_tx: outgoing_tx.clone(),
        command_rx: Mutex::new(command_rx),
    });

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("bridge runtime");
        rt.block_on(serve(state_rx, outgoing_tx, command_tx));
    });
}

async fn serve(
    state_rx: watch::Receiver<String>,
    outgoing_tx: broadcast::Sender<String>,
    command_tx: mpsc::UnboundedSender<ExtensionCommand>,
) {
    let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{PORT}")).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[bridge] bind :{PORT} failed: {e}");
            return;
        }
    };
    eprintln!("[bridge] ws://127.0.0.1:{PORT}");

    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
            continue;
        };

        let mut outgoing_rx = outgoing_tx.subscribe();
        let cmd_tx = command_tx.clone();
        let current_state = state_rx.borrow().clone();

        tokio::spawn(async move {
            use tokio_tungstenite::tungstenite::Message;
            let (mut sink, mut stream) = ws.split();

            let status = serde_json::json!({"type": "status", "state": current_state});
            let _ = sink.send(Message::Text(status.to_string().into())).await;

            loop {
                tokio::select! {
                    msg = outgoing_rx.recv() => {
                        match msg {
                            Ok(json) => {
                                if sink.send(Message::Text(json.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    frame = stream.next() => {
                        match frame {
                            Some(Ok(Message::Text(text))) => {
                                if text.contains("toggle") {
                                    let _ = cmd_tx.send(ExtensionCommand::ToggleListening);
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            _ => {}
                        }
                    }
                }
            }
        });
    }
}
