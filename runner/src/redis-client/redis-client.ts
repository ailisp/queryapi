import { createClient, type RedisClientType } from 'redis';

interface StreamMessage {
  id: string
  message: {
    block_height: string
  }
}

interface StreamStorage {
  account_id: string
  function_name: string
  code: string
  schema: string
}

export type StreamType = 'historical' | 'real-time';

export default class RedisClient {
  SMALLEST_STREAM_ID = '0';
  LARGEST_STREAM_ID = '+';
  STREAMS_SET_KEY = 'streams';
  STREAMER_MESSAGE_HASH_KEY_BASE = 'streamer:message:';

  constructor (
    private readonly client: RedisClientType = createClient({ url: process.env.REDIS_CONNECTION_STRING })
  ) {
    client.on('error', (err) => { console.log('Redis Client Error', err); });
    client.connect().catch(console.error);
  }

  private generateStorageKey (streamkey: string): string {
    return `${streamkey}:storage`;
  };

  getStreamType (streamKey: string): StreamType {
    if (streamKey.endsWith(':historical:stream')) {
      return 'historical';
    }
    return 'real-time';
  }

  async disconnect (): Promise<void> {
    await this.client.disconnect();
  }

  async getStreamMessages (
    streamKey: string,
    streamId = this.SMALLEST_STREAM_ID,
    count = 1
  ): Promise<StreamMessage[] | null> {
    const results = await this.client.xRead(
      { key: streamKey, id: streamId },
      { COUNT: count }
    );

    return results?.[0].messages as StreamMessage[];
  };

  async deleteStreamMessage (
    streamKey: string,
    id: string,
  ): Promise<void> {
    await this.client.xDel(streamKey, id);
  };

  async getUnprocessedStreamMessages (
    streamKey: string,
    startId = this.SMALLEST_STREAM_ID,
  ): Promise<StreamMessage[]> {
    const results = await this.client.xRange(streamKey, startId, this.LARGEST_STREAM_ID);

    return results as StreamMessage[];
  };

  async getStreamStorage (streamKey: string): Promise<StreamStorage> {
    const storageKey = this.generateStorageKey(streamKey);
    const results = await this.client.get(storageKey);

    if (results === null) {
      throw new Error(`${storageKey} does not have any data`);
    }

    return JSON.parse(results);
  };

  async getStreams (): Promise<string[]> {
    return await this.client.sMembers(this.STREAMS_SET_KEY);
  }

  async getStreamerMessage (blockHeight: number): Promise<string | null> {
    return await this.client.get(`${this.STREAMER_MESSAGE_HASH_KEY_BASE}${blockHeight}`);
  }
}
