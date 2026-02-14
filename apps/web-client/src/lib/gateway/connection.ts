/**
 * WebSocket Gateway connection manager.
 *
 * Handles the full lifecycle:
 *   connect → IDENTIFY → READY → heartbeat loop → dispatch events → reconnect
 */

import {
  Opcode,
  type ClientMessage,
  type GatewayMessage,
  type ReadyPayload,
  type DispatchEventName,
} from "@/types/gateway";
import { handleDispatch } from "@/lib/gateway/handler";

// ---------------------------------------------------------------------------
// Connection status
// ---------------------------------------------------------------------------

export type GatewayStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "reconnecting";

type StatusListener = (status: GatewayStatus) => void;

// ---------------------------------------------------------------------------
// Reconnect backoff constants
// ---------------------------------------------------------------------------

const BACKOFF_INITIAL_MS = 1_000;
const BACKOFF_MAX_MS = 30_000;
const BACKOFF_FACTOR = 2;

// ---------------------------------------------------------------------------
// GatewayConnection
// ---------------------------------------------------------------------------

export class GatewayConnection {
  readonly podId: string;

  private ws: WebSocket | null = null;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private heartbeatInterval = 41_250;
  private seq = 0;

  // Reconnection state
  private wsUrl: string | null = null;
  private wsTicket: string | null = null;
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private intentionalClose = false;

  // Status tracking
  private _status: GatewayStatus = "disconnected";
  private statusListeners = new Set<StatusListener>();

  constructor(podId: string) {
    this.podId = podId;
  }

  /** Current connection status. */
  get status(): GatewayStatus {
    return this._status;
  }

  /** Subscribe to status changes. Returns an unsubscribe function. */
  onStatusChange(listener: StatusListener): () => void {
    this.statusListeners.add(listener);
    return () => this.statusListeners.delete(listener);
  }

  private setStatus(status: GatewayStatus) {
    if (this._status === status) return;
    this._status = status;
    for (const listener of this.statusListeners) {
      listener(status);
    }
  }

  // -----------------------------------------------------------------------
  // Public API
  // -----------------------------------------------------------------------

  /**
   * Open a WebSocket connection and authenticate with the given ticket.
   * Resolves once READY is received.
   */
  connect(wsUrl: string, wsTicket: string): Promise<ReadyPayload> {
    this.wsUrl = wsUrl;
    this.wsTicket = wsTicket;
    this.intentionalClose = false;
    this.reconnectAttempt = 0;

    return this.doConnect(wsUrl, wsTicket);
  }

  /**
   * Gracefully close the connection. Does NOT trigger auto-reconnect.
   */
  disconnect() {
    this.intentionalClose = true;
    this.cleanup();
    this.setStatus("disconnected");
  }

  // -----------------------------------------------------------------------
  // Connection internals
  // -----------------------------------------------------------------------

  private doConnect(wsUrl: string, wsTicket: string): Promise<ReadyPayload> {
    return new Promise((resolve, reject) => {
      this.cleanup();

      const isReconnect = this.reconnectAttempt > 0;
      this.setStatus(isReconnect ? "reconnecting" : "connecting");

      const url = `${wsUrl}?v=1&encoding=json`;
      const ws = new WebSocket(url);
      this.ws = ws;

      ws.onopen = () => {
        // Send IDENTIFY immediately.
        const identify: ClientMessage = {
          op: Opcode.IDENTIFY,
          d: { ticket: wsTicket },
        };
        ws.send(JSON.stringify(identify));
      };

      let ready = false;

      ws.onmessage = (event) => {
        const raw: GatewayMessage = JSON.parse(event.data as string);

        switch (raw.op) {
          case Opcode.DISPATCH: {
            const eventName = raw.t as DispatchEventName;
            this.seq = raw.s ?? this.seq;

            if (eventName === "READY") {
              const payload = raw.d as ReadyPayload;
              this.heartbeatInterval = payload.heartbeat_interval;
              this.startHeartbeat();
              this.reconnectAttempt = 0;
              this.setStatus("connected");
              ready = true;
              resolve(payload);
            } else {
              handleDispatch(eventName, raw.d, this.podId);
            }
            break;
          }

          case Opcode.HEARTBEAT_ACK: {
            // Server acknowledged our heartbeat — nothing to do.
            break;
          }

          default:
            console.warn("[gateway] Unknown opcode:", raw.op);
        }
      };

      ws.onerror = (event) => {
        console.error("[gateway] WebSocket error:", event);
        if (!ready) {
          reject(new Error("WebSocket connection error"));
        }
      };

      ws.onclose = (event) => {
        console.info(
          `[gateway] Connection closed: code=${event.code} reason="${event.reason}"`,
        );
        this.stopHeartbeat();

        if (!ready) {
          reject(new Error(event.reason || `WebSocket closed (${event.code})`));
          return;
        }

        if (!this.intentionalClose) {
          this.setStatus("reconnecting");
          this.scheduleReconnect();
        } else {
          this.setStatus("disconnected");
        }
      };
    });
  }

  // -----------------------------------------------------------------------
  // Heartbeat
  // -----------------------------------------------------------------------

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      this.sendHeartbeat();
    }, this.heartbeatInterval);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private sendHeartbeat() {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    const msg: ClientMessage = {
      op: Opcode.HEARTBEAT,
      d: { seq: this.seq },
    };
    this.ws.send(JSON.stringify(msg));
  }

  // -----------------------------------------------------------------------
  // Reconnect with exponential backoff
  // -----------------------------------------------------------------------

  private scheduleReconnect() {
    if (this.intentionalClose || !this.wsUrl || !this.wsTicket) return;

    const delay = Math.min(
      BACKOFF_INITIAL_MS * Math.pow(BACKOFF_FACTOR, this.reconnectAttempt),
      BACKOFF_MAX_MS,
    );
    this.reconnectAttempt++;

    console.info(
      `[gateway] Reconnecting in ${delay}ms (attempt ${this.reconnectAttempt})...`,
    );

    this.reconnectTimer = setTimeout(() => {
      if (this.intentionalClose) return;
      const url = this.wsUrl;
      const ticket = this.wsTicket;
      if (!url || !ticket) return;
      this.doConnect(url, ticket).catch((err) => {
        console.warn("[gateway] Reconnect failed:", err);
        if (!this.intentionalClose) {
          this.scheduleReconnect();
        }
      });
    }, delay);
  }

  // -----------------------------------------------------------------------
  // Cleanup
  // -----------------------------------------------------------------------

  private cleanup() {
    this.stopHeartbeat();

    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }

    if (this.ws) {
      // Remove event handlers to prevent spurious reconnects.
      this.ws.onopen = null;
      this.ws.onmessage = null;
      this.ws.onerror = null;
      this.ws.onclose = null;

      if (
        this.ws.readyState === WebSocket.OPEN ||
        this.ws.readyState === WebSocket.CONNECTING
      ) {
        this.ws.close(1000, "Client disconnect");
      }
      this.ws = null;
    }
  }
}
