// voice-typing — background service worker
// Maintains a WebSocket connection to the local voice-typing app and relays state
// to content scripts running in every tab.

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
    // Server sends current state on connect; default to idle until then.
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

  ws.onerror = () => {
    // onclose fires right after — reconnect is handled there.
  };
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
    const tabs = await chrome.tabs.query({});
    for (const tab of tabs) {
      if (tab.id) chrome.tabs.sendMessage(tab.id, msg).catch(() => {});
    }
  } catch {
    /* extension context may be invalidated */
  }
}

async function broadcastToActiveTabs(msg) {
  try {
    const tabs = await chrome.tabs.query({ active: true, currentWindow: true });
    for (const tab of tabs) {
      if (tab.id) chrome.tabs.sendMessage(tab.id, msg).catch(() => {});
    }
  } catch {}
}

// Messages from content scripts
chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
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

// Boot
connect();
