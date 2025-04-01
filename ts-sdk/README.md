# Titan Indexer Client SDK for TypeScript

This package provides a TypeScript client for interacting with the [Titan Indexer](https://github.com/titan-io/titan-indexer) for Bitcoin. While the indexer itself is written in Rust, this SDK offers convenient HTTP and TCP clients for TypeScript applications to communicate with the indexer.

The HTTP client uses [axios](https://axios-http.com/) to call REST API endpoints, and the TCP client (which works only in Node.js) uses Node's built-in `net` module to subscribe to real-time events.

---

## Features

- **HTTP Client**:
  - Communicate with endpoints like `/status`, `/tip`, `/tx/:txid`, `/address/:address`, etc.
  - Easily fetch block data, transaction details, inscriptions, and more.
- **TCP Client**:
  - Subscribe to events (e.g. `RuneEtched`, `RuneMinted`, etc.) via a TCP socket.
  - Built-in auto-reconnection logic to handle disconnections gracefully.
  - **Note**: This client uses Node's `net` module and will only work in Node.js (not in browser environments).

---

## Requirements

- **Node.js**: Required for the TCP client since it depends on Node's `net` module.
- **axios**: Used for making HTTP requests.
- **TypeScript**: For type safety and development.
- Bitcoin Node 27.0 (https://bitcoincore.org/bin/bitcoin-core-27.0/)
- A running instance of the Titan Indexer (HTTP on a specified port and TCP for event subscriptions).

Detailed Setup: Follow the [Setup Instructions](../SetupInstructions.md) for step-by-step guidance.

---

## Installation

Install the package via npm or yarn:

```bash
npm install titan-client
# or
yarn add titan-client
```

## Usage

### HTTP Client

The HTTP client uses axios to communicate with the Titan Indexer's REST API endpoints. Create an instance of TitanHttpClient by passing the base URL of your Titan Indexer service and call the available methods.

#### Example

```typescript
import { TitanHttpClient } from 'titan-client';

async function testHttpClient() {
  // Create an HTTP client instance.
  const httpClient = new TitanHttpClient('http://localhost:3030');

  try {
    // Retrieve the node status.
    const status = await httpClient.getStatus();
    console.log('Index Status:', status);

    // Retrieve the current block tip.
    const tip = await httpClient.getTip();
    console.log('Block Tip:', tip);

    // Fetch address data for a given Bitcoin address.
    const addressData = await httpClient.getAddress('your-bitcoin-address');
    console.log('Address Data:', addressData);

    // Fetch a transaction by txid.
    const transaction = await httpClient.getTransaction('txid-here');
    console.log('Transaction:', transaction);
  } catch (error) {
    console.error('HTTP Client Error:', error);
  }
}

testHttpClient();
```

### TCP Client

The TCP client allows you to subscribe to real-time events from the Titan Indexer. It features automatic reconnection logic on disconnection.

**Important**: The TCP client works only in Node.js since it depends on Node's `net` module.

#### Example

```typescript
import { TitanTcpClient } from 'titan-client';

function testTcpSubscription() {
  // Create a TCP client instance with auto-reconnect enabled.
  const tcpClient = new TitanTcpClient('localhost', 4000, {
    autoReconnect: true, // Automatically reconnect if disconnected
    reconnectDelayMs: 5000, // Wait 5 seconds between reconnection attempts
  });

  // Listen for incoming events.
  tcpClient.on('event', (event) => {
    console.log('Received event:', event);
  });

  // Listen for errors.
  tcpClient.on('error', (err) => {
    console.error('TCP Client Error:', err);
  });

  // Listen for when the connection closes.
  tcpClient.on('close', () => {
    console.log('TCP connection closed.');
  });

  // Listen for reconnection notifications.
  tcpClient.on('reconnect', () => {
    console.log('TCP client reconnected.');
  });

  // Start the subscription.
  tcpClient.subscribe({ subscribe: ['RuneEtched', 'RuneMinted'] });

  // Optionally, shut down the client gracefully after 30 seconds.
  setTimeout(() => {
    tcpClient.shutdown();
    console.log('TCP client shut down.');
  }, 30000);
}

testTcpSubscription();
```

## API Reference

### HTTP Client (TitanHttpClient)

- **getStatus()**: `Promise<Status>`
  Retrieves the node's status (network info, block height, etc.).

- **getTip()**: `Promise<BlockTip>`
  Retrieves the current best block tip.

- **getBlock(query: string)**: `Promise<any>`
  Fetches a block by its height or hash.

- **getBlockHashByHeight(height: number)**: `Promise<string>`
  Returns the block hash for a given height.

- **getBlockTxids(query: string)**: `Promise<string[]>`
  Retrieves a list of transaction IDs for a block.

- **getAddress(address: string)**: `Promise<AddressData>`
  Retrieves address data including balance and transaction outputs.

- **getTransaction(txid: string)**: `Promise<Transaction>`
  Retrieves detailed information for a given transaction.

- **getTransactionRaw(txid: string)**: `Promise<Uint8Array>`
  Retrieves the raw binary data of a transaction.

- **getTransactionHex(txid: string)**: `Promise<string>`
  Retrieves the raw transaction hex.

- **sendTransaction(txHex: string)**: `Promise<string>`
  Broadcasts a raw transaction hex to the network.

- **getOutput(outpoint: string)**: `Promise<TxOutEntry>`
  Retrieves data for a specific transaction output.

- **getInscription(inscriptionId: string)**: `Promise<{ headers: any; data: Uint8Array }>`
  Retrieves inscription headers and data.

- **getRunes(pagination?: Pagination)**: `Promise<PaginationResponse<RuneResponse>>`
  Retrieves a paginated list of runes.

- **getRune(rune: string)**: `Promise<RuneResponse>`
  Retrieves data for a specific rune.

- **getRuneTransactions(rune: string, pagination?: Pagination)**: `Promise<PaginationResponse<string>>`
  Retrieves a paginated list of transaction IDs involving a specific rune.

- **getMempoolTxids()**: `Promise<string[]>`
  Retrieves the transaction IDs currently in the mempool.

- **getMempoolEntry(txid: string)**: `Promise<MempoolEntry>`
  Retrieves a specific mempool entry by its txid.

- **getMempoolEntries(txids: string[])**: `Promise<Map<string, MempoolEntry>>`
  Retrieves multiple mempool entries by their txids.

- **getAllMempoolEntries()**: `Promise<Map<string, MempoolEntry>>`
  Retrieves all mempool entries.

- **getSubscription(id: string)**: `Promise<Subscription>`
  Retrieves a subscription by its ID.

- **listSubscriptions()**: `Promise<Subscription[]>`
  Lists all subscriptions.

- **addSubscription(subscription: Subscription)**: `Promise<Subscription>`
  Adds a new subscription.

- **deleteSubscription(id: string)**: `Promise<void>`
  Deletes a subscription by its ID.

### TCP Client (TitanTcpClient)

#### Events

- **event**:
  Emitted when a new event is received from the indexer.

- **error**:
  Emitted when an error occurs with the TCP connection.

- **close**:
  Emitted when the TCP connection is closed.

- **reconnect**:
  Emitted when the client reconnects after a disconnection.

#### Methods

- **subscribe(subscriptionRequest: TcpSubscriptionRequest)**: `void`
  Initiates the subscription process using the provided subscription request. The request is stored so it can be re-sent on reconnection.

- **shutdown()**: `void`
  Gracefully shuts down the TCP client and cancels any pending reconnection attempts.
