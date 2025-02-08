export interface BlockTip {
  height: number;
  hash: string;
}

export interface Status {
  block_tip: BlockTip;
  runes_count: number;
  mempool_tx_count: number;
}

export interface RuneAmount {
  rune_id: string;
  amount: string;
}

export interface TransactionStatus {
  confirmed: boolean;
  block_height?: number;
  block_hash?: string;
}

export interface AddressTxOut {
  outpoint: string;
  value: number;
  runes: RuneAmount[];
  status?: TransactionStatus;
}

export interface AddressData {
  value: number;
  runes: RuneAmount[];
  outputs: AddressTxOut[];
}

export interface TxOut {
  value: number;
  script_pubkey: string;
  runes: RuneAmount[];
}

export interface TxOutEntry {
  runes: RuneAmount[];
  value: number;
  spent: boolean;
}

export interface Transaction {
  version: number;
  lock_time: number;
  input: any[]; // You may wish to further define the TxIn type.
  output: TxOut[];
  status?: TransactionStatus;
}

export interface MintResponse {
  start?: number;
  end?: number;
  mintable: boolean;
  cap: string;
  amount: string;
  mints: string;
}

export interface RuneResponse {
  id: string;
  block: number;
  burned: string;
  divisibility: number;
  etching: string;
  number: number;
  premine: string;
  supply: string;
  max_supply: string;
  spaced_rune: string;
  symbol?: string;
  mint?: MintResponse;
  pending_burns: string;
  pending_mints: string;
  inscription_id?: string;
  timestamp: number;
  turbo: boolean;
}

export interface Subscription {
  id: string;
  endpoint: string;
  event_types: string[];
  last_success_epoch_secs: number;
}

export interface Pagination {
  skip?: number;
  limit?: number;
}

export interface PaginationResponse<T> {
  items: T[];
  offset: number;
}

export enum EventType {
  RuneEtched = 'RuneEtched',
  RuneMinted = 'RuneMinted',
  RuneBurned = 'RuneBurned',
  RuneTransferred = 'RuneTransferred',
  AddressModified = 'AddressModified',
  TransactionsAdded = 'TransactionsAdded',
  TransactionsReplaced = 'TransactionsReplaced',
  NewBlock = 'NewBlock',
}

/**
 * The request object to subscribe to TCP events.
 * For example, a client might send:
 *   { subscribe: ["RuneEtched", "RuneMinted"] }
 */
export interface TcpSubscriptionRequest {
  subscribe: EventType[];
}
