import express from 'express';
import { Gauge, Histogram, Counter, AggregatorRegistry } from 'prom-client';

const BLOCK_WAIT_DURATION = new Gauge({
  name: 'queryapi_runner_block_wait_duration_milliseconds',
  help: 'Time an indexer function waited for a block before processing',
  labelNames: ['indexer', 'type'],
});

const CACHE_HIT = new Counter({
  name: 'queryapi_runner_cache_hit',
  help: 'The number of times cache was hit successfully'
});

const CACHE_MISS = new Counter({
  name: 'queryapi_runner_cache_miss',
  help: 'The number of times cache was missed'
});

const UNPROCESSED_STREAM_MESSAGES = new Gauge({
  name: 'queryapi_runner_unprocessed_stream_messages',
  help: 'Number of Redis Stream messages not yet processed',
  labelNames: ['indexer', 'type'],
});

const LAST_PROCESSED_BLOCK_HEIGHT = new Gauge({
  name: 'queryapi_runner_last_processed_block_height',
  help: 'Previous block height processed by an indexer',
  labelNames: ['indexer', 'type'],
});

const EXECUTION_DURATION = new Histogram({
  name: 'queryapi_runner_execution_duration_milliseconds',
  help: 'Time taken to execute an indexer function',
  labelNames: ['indexer', 'type'],
});

export const METRICS = {
  BLOCK_WAIT_DURATION,
  CACHE_HIT,
  CACHE_MISS,
  UNPROCESSED_STREAM_MESSAGES,
  LAST_PROCESSED_BLOCK_HEIGHT,
  EXECUTION_DURATION,
};

const aggregatorRegistry = new AggregatorRegistry();
const workerMetrics: Record<number, string> = {};

export const registerWorkerMetrics = (workerId: number, metrics: string): void => {
  workerMetrics[workerId] = metrics;
};

export const startServer = async (): Promise<void> => {
  const app = express();

  // https://github.com/DefinitelyTyped/DefinitelyTyped/issues/50871
  // eslint-disable-next-line @typescript-eslint/no-misused-promises
  app.get('/metrics', async (_req, res) => {
    res.set('Content-Type', aggregatorRegistry.contentType);

    const metrics = await AggregatorRegistry.aggregate(Object.values(workerMetrics)).metrics();
    res.send(metrics);
  });

  app.listen(process.env.PORT, () => {
    console.log(`Metrics server running on http://localhost:${process.env.PORT}`);
  });
};
