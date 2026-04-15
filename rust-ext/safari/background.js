// voice-typing — background service worker (Safari)
// Maintains a WebSocket connection to the local voice-typing app and relays state
// to content scripts running in every tab.

const api = typeof browser !== "undefined" ? browser : chrome;
const WS_URL = "ws://127.0.0.1:36199";

let ws = null;
let state = "disconnected";
let reconnectTimer = null;

function connect() {
  if (ws && ws.readyState <= WebSocket.OPEN) return;

  try {
    ws = new WebSocket(WS_URL);
  } catch {
    scheduleReconnect();
    return;
  }

  ws.onopen = () => {
    if (state === "disconnected") {
      state = "idle";
      broadcastToTabs({ type: "stateUpdate", state });
    }
  };

  ws.onmessage = (event) => {
    try {
      const msg = JSON.parse(event.data);
      if (msg.type === "status") {
        state = msg.state;
        broadcastToTabs({ type: "stateUpdate", state });
      } else if (msg.type === "transcript") {
        broadcastToActiveTabs({
          type: "transcript",
          text: msg.text,
          is_final: msg.is_final,
        });
      }
    } catch {
      /* ignore malformed frames */
    }
  };

  ws.onclose = () => {
    ws = null;
    state = "disconnected";
    broadcastToTabs({ type: "stateUpdate", state });
    scheduleReconnect();
  };

  ws.onerror = () => {};
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, 2000);
}

async function broadcastToTabs(msg) {
  try {
    const tabs = await api.tabs.query({});
    for (const tab of tabs) {
      if (tab.id) api.tabs.sendMessage(tab.id, msg).catch(() => {});
    }
  } catch {}
}

async function broadcastToActiveTabs(msg) {
  try {
    const tabs = await api.tabs.query({ active: true, currentWindow: true });
    for (const tab of tabs) {
      if (tab.id) api.tabs.sendMessage(tab.id, msg).catch(() => {});
    }
  } catch {}
}

api.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (msg.type === "getState") {
    sendResponse({ state });
    return true;
  }

  if (msg.type === "toggle") {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: "toggle" }));
    } else {
      connect();
    }
    return true;
  }
});

connect();
