import * as net from 'net';
import { EventEmitter } from 'events';
import * as readline from 'readline';
import { TcpSubscriptionRequest } from './types';

/**
 * Options for the TCP client.
 */
export interface TcpClientOptions {
  autoReconnect?: boolean;    // Should the client attempt to reconnect on disconnection?
  reconnectDelayMs?: number;  // Delay before attempting to reconnect (in milliseconds)
}

/**
 * A TCP client class for subscribing to events from the Titan service.
 * This version automatically attempts to reconnect if the connection is lost.
 *
 * It emits:
 *   - "event": when a new event is received.
 *   - "error": when an error occurs.
 *   - "close": when the connection is closed.
 *   - "reconnect": when a reconnection attempt is made.
 *
 * Usage example:
 *
 *   const tcpClient = new TitanTcpClient('localhost', 4000, { autoReconnect: true, reconnectDelayMs: 5000 });
 *   tcpClient.on('event', (event) => console.log('Received event:', event));
 *   tcpClient.subscribe({ subscribe: ['RuneEtched', 'RuneMinted'] });
 *
 *   // To shut down:
 *   tcpClient.shutdown();
 */
export class TitanTcpClient extends EventEmitter {
  private socket: net.Socket | null = null;
  private rl: readline.Interface | null = null;
  private subscriptionRequest: TcpSubscriptionRequest | null = null;
  private shuttingDown = false;

  // Options for reconnection behavior.
  private autoReconnect: boolean;
  private reconnectDelayMs: number;

  /**
   * @param addr The hostname or IP address of the TCP subscription server.
   * @param port The port number of the TCP subscription server.
   * @param options Optional configuration for reconnection behavior.
   */
  constructor(
    private addr: string,
    private port: number,
    options?: TcpClientOptions
  ) {
    super();
    this.autoReconnect = options?.autoReconnect ?? false;
    this.reconnectDelayMs = options?.reconnectDelayMs ?? 5000;
  }

  /**
   * Initiates the subscription process. The provided subscription request will be
   * stored and used for all (re)connections.
   *
   * @param subscriptionRequest The subscription request (e.g. { subscribe: ['RuneEtched', 'RuneMinted'] }).
   */
  subscribe(subscriptionRequest: TcpSubscriptionRequest): void {
    this.subscriptionRequest = subscriptionRequest;
    this.shuttingDown = false;
    this.connect();
  }

  /**
   * Creates a TCP connection and sets up event listeners.
   */
  private connect(): void {
    if (!this.subscriptionRequest) {
      this.emit('error', new Error('No subscription request provided.'));
      return;
    }

    // Create a new TCP connection.
    this.socket = net.createConnection(this.port, this.addr, () => {
      // On connection, send the subscription request as JSON (terminated by newline).
      const reqJson = JSON.stringify(this.subscriptionRequest);
      this.socket?.write(reqJson + "\n");
      this.emit('reconnect'); // notify that a (re)connection has occurred.
    });

    this.socket.on('error', (err) => {
      this.emit('error', err);
    });

    // Set up a readline interface to handle incoming lines.
    this.rl = readline.createInterface({ input: this.socket });
    this.rl.on('line', (line: string) => {
      try {
        const event = JSON.parse(line);
        this.emit('event', event);
      } catch (err) {
        this.emit('error', new Error(`Failed to parse event: ${line}`));
      }
    });

    // Handle connection closure.
    this.socket.on('close', () => {
      this.emit('close');
      this.cleanup();

      // If the client has not been explicitly shut down and autoReconnect is enabled,
      // try to reconnect after a delay.
      if (!this.shuttingDown && this.autoReconnect) {
        setTimeout(() => {
          this.connect();
        }, this.reconnectDelayMs);
      }
    });
  }

  /**
   * Cleans up the socket and readline interface.
   */
  private cleanup(): void {
    if (this.rl) {
      this.rl.close();
      this.rl = null;
    }
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }
  }

  /**
   * Shuts down the TCP client, canceling any pending reconnection attempts.
   */
  shutdown(): void {
    this.shuttingDown = true;
    this.cleanup();
  }
}
