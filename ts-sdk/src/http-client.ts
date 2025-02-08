import axios, { AxiosInstance } from 'axios';
import {
  AddressData,
  BlockTip,
  Pagination,
  PaginationResponse,
  RuneResponse,
  Subscription,
  Status,
  Transaction,
  TxOutEntry,
  TransactionStatus,
} from './types';

/**
 * A HTTP client class for the Titan API. Each method corresponds
 * to an endpoint (for example: GET /status, GET /tx/:txid, POST /tx/broadcast, etc.)
 */
export class TitanHttpClient {
  private http: AxiosInstance;
  private baseUrl: string;

  constructor(baseUrl: string) {
    // Remove any trailing slashes from the URL.
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.http = axios.create({
      baseURL: this.baseUrl,
    });
  }

  async getStatus(): Promise<Status> {
    const response = await this.http.get<Status>('/status');
    return response.data;
  }

  async getTip(): Promise<BlockTip> {
    const response = await this.http.get<BlockTip>('/tip');
    return response.data;
  }

  /**
   * Fetches a block by its query (could be a block height or hash).
   */
  async getBlock(query: string): Promise<any> {
    const response = await this.http.get<any>(`/block/${query}`);
    return response.data;
  }

  async getBlockHashByHeight(height: number): Promise<string> {
    const response = await this.http.get<string>(`/block/${height}/hash`);
    return response.data;
  }

  async getBlockTxids(query: string): Promise<string[]> {
    const response = await this.http.get<string[]>(`/block/${query}/txids`);
    return response.data;
  }

  async getAddress(address: string): Promise<AddressData> {
    const response = await this.http.get<AddressData>(`/address/${address}`);
    return response.data;
  }

  async getTransaction(txid: string): Promise<Transaction> {
    const response = await this.http.get<Transaction>(`/tx/${txid}`);
    return response.data;
  }

  async getTransactionRaw(txid: string): Promise<Uint8Array> {
    // Request raw binary data using the arraybuffer responseType.
    const response = await this.http.get<ArrayBuffer>(`/tx/${txid}/raw`, {
      responseType: 'arraybuffer',
    });
    return new Uint8Array(response.data);
  }

  async getTransactionHex(txid: string): Promise<string> {
    const response = await this.http.get<string>(`/tx/${txid}/hex`);
    return response.data;
  }

  async getTransactionStatus(txid: string): Promise<TransactionStatus> {
    const response = await this.http.get<TransactionStatus>(
      `/tx/${txid}/status`,
    );
    return response.data;
  }

  async sendTransaction(txHex: string): Promise<string> {
    const response = await this.http.post<string>('/tx/broadcast', txHex, {
      headers: {
        'Content-Type': 'text/plain',
      },
    });
    // Assume the response body is the transaction id.
    return response.data;
  }

  async getOutput(outpoint: string): Promise<TxOutEntry> {
    const response = await this.http.get<TxOutEntry>(`/output/${outpoint}`);
    return response.data;
  }

  async getInscription(
    inscriptionId: string,
  ): Promise<{ headers: any; data: Uint8Array }> {
    const response = await this.http.get<ArrayBuffer>(
      `/inscription/${inscriptionId}`,
      {
        responseType: 'arraybuffer',
      },
    );
    return {
      headers: response.headers,
      data: new Uint8Array(response.data),
    };
  }

  async getRunes(
    pagination?: Pagination,
  ): Promise<PaginationResponse<RuneResponse>> {
    const params = pagination || {};
    const response = await this.http.get<PaginationResponse<RuneResponse>>(
      '/runes',
      { params },
    );
    return response.data;
  }

  async getRune(rune: string): Promise<RuneResponse> {
    const response = await this.http.get<RuneResponse>(`/rune/${rune}`);
    return response.data;
  }

  async getRuneTransactions(
    rune: string,
    pagination?: Pagination,
  ): Promise<PaginationResponse<string>> {
    const params = pagination || {};
    const response = await this.http.get<PaginationResponse<string>>(
      `/rune/${rune}/transactions`,
      { params },
    );
    return response.data;
  }

  async getMempoolTxids(): Promise<string[]> {
    const response = await this.http.get<string[]>('/mempool/txids');
    return response.data;
  }

  async getSubscription(id: string): Promise<Subscription> {
    const response = await this.http.get<Subscription>(`/subscription/${id}`);
    return response.data;
  }

  async listSubscriptions(): Promise<Subscription[]> {
    const response = await this.http.get<Subscription[]>('/subscriptions');
    return response.data;
  }

  async addSubscription(subscription: Subscription): Promise<Subscription> {
    const response = await this.http.post<Subscription>(
      '/subscription',
      subscription,
    );
    return response.data;
  }

  async deleteSubscription(id: string): Promise<void> {
    const response = await this.http.delete(`/subscription/${id}`);
    if (response.status < 200 || response.status >= 300) {
      throw new Error(`Delete subscription failed: HTTP ${response.status}`);
    }
  }
}
