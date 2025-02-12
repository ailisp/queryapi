use std::collections::HashMap;

use futures::stream::{self, StreamExt};
use near_jsonrpc_client::JsonRpcClient;
use tokio::sync::Mutex;

use indexer_rules_engine::types::indexer_rule_match::{ChainId, IndexerRuleMatch};
use near_lake_framework::near_indexer_primitives::types::BlockHeight;
use near_lake_framework::near_indexer_primitives::StreamerMessage;
use utils::serialize_to_camel_case_json_string;

use crate::indexer_types::IndexerFunction;
use indexer_types::IndexerRegistry;
use opts::{Opts, Parser};
use storage::{self, generate_real_time_streamer_message_key, ConnectionManager};

mod historical_block_processing;
mod indexer_reducer;
mod indexer_registry;
mod indexer_types;
mod metrics;
mod opts;
mod s3;
mod utils;

pub(crate) const INDEXER: &str = "queryapi_coordinator";

type SharedIndexerRegistry = std::sync::Arc<Mutex<IndexerRegistry>>;

type Streamers = std::sync::Arc<Mutex<HashMap<String, historical_block_processing::Streamer>>>;

pub(crate) struct QueryApiContext<'a> {
    pub streamer_message: near_lake_framework::near_indexer_primitives::StreamerMessage,
    pub chain_id: &'a ChainId,
    pub s3_client: &'a aws_sdk_s3::Client,
    pub json_rpc_client: &'a JsonRpcClient,
    pub registry_contract_id: &'a str,
    pub redis_connection_manager: &'a ConnectionManager,
    pub indexer_registry: &'a SharedIndexerRegistry,
    pub streamers: &'a Streamers,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    opts::init_tracing();

    opts::dotenv::dotenv().ok();

    let opts = Opts::parse();

    let chain_id = &opts.chain_id();
    let registry_contract_id = opts.registry_contract_id.clone();

    let aws_config = &opts.lake_aws_sdk_config();
    let s3_config = aws_sdk_s3::config::Builder::from(aws_config).build();
    let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

    tracing::info!(target: INDEXER, "Connecting to redis...");
    let redis_connection_manager = storage::connect(&opts.redis_connection_string).await?;

    let json_rpc_client = JsonRpcClient::connect(opts.rpc_url());

    // fetch raw indexer functions for use in indexer
    // Could this give us results from a newer block than the next block we receive from the Lake?
    tracing::info!(
        target: INDEXER,
        "Fetching indexer functions from contract registry..."
    );
    let indexer_functions = indexer_registry::read_indexer_functions_from_registry(
        &json_rpc_client,
        &registry_contract_id,
    )
    .await;
    let indexer_functions = indexer_registry::build_registry_from_json(indexer_functions);
    let indexer_registry: SharedIndexerRegistry =
        std::sync::Arc::new(Mutex::new(indexer_functions));

    let streamers = std::sync::Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    tracing::info!(target: INDEXER, "Generating LakeConfig...");
    let config: near_lake_framework::LakeConfig = opts.to_lake_config().await;

    tracing::info!(target: INDEXER, "Instantiating the stream...",);
    let (sender, stream) = near_lake_framework::streamer(config);

    tokio::spawn(utils::stats(redis_connection_manager.clone()));
    tokio::spawn(metrics::init_server(opts.port).expect("Failed to start metrics server"));

    tracing::info!(target: INDEXER, "Starting queryapi_coordinator...",);
    let mut handlers = tokio_stream::wrappers::ReceiverStream::new(stream)
        .map(|streamer_message| {
            let context = QueryApiContext {
                redis_connection_manager: &redis_connection_manager,
                registry_contract_id: &registry_contract_id,
                streamer_message,
                chain_id,
                json_rpc_client: &json_rpc_client,
                s3_client: &s3_client,
                indexer_registry: &indexer_registry,
                streamers: &streamers,
            };

            handle_streamer_message(context)
        })
        .buffer_unordered(1usize);

    while let Some(handle_message) = handlers.next().await {
        if let Err(err) = handle_message {
            tracing::error!(target: INDEXER, "{:#?}", err);
        }
    }
    drop(handlers); // close the channel so the sender will stop

    // propagate errors from the sender
    match sender.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(anyhow::Error::from(e)), // JoinError
    }
}

async fn handle_streamer_message(context: QueryApiContext<'_>) -> anyhow::Result<u64> {
    let indexer_functions = {
        let lock = context.indexer_registry.lock().await;

        lock.values()
            .flat_map(|fns| fns.values())
            .cloned()
            .collect::<Vec<_>>()
    };

    let mut indexer_function_filter_matches_futures = stream::iter(indexer_functions.iter())
        .map(|indexer_function| {
            reduce_rule_matches_for_indexer_function(
                indexer_function,
                &context.streamer_message,
                context.chain_id.clone(),
            )
        })
        // TODO: fix the buffer size used to accumulate results, it takes 10 vecs of vecs while we want to take 10 IndexerRuleMatches
        .buffer_unordered(10usize);

    let block_height: BlockHeight = context.streamer_message.block.header.height;

    // Cache streamer message block and shards for use in real time processing
    storage::set(
        context.redis_connection_manager,
        generate_real_time_streamer_message_key(block_height),
        &serialize_to_camel_case_json_string(&context.streamer_message)?,
        Some(60),
    )
    .await?;

    indexer_registry::index_registry_changes(block_height, &context).await?;

    while let Some(indexer_function_with_matches) =
        indexer_function_filter_matches_futures.next().await
    {
        if let Ok(indexer_function_with_matches) = indexer_function_with_matches {
            let indexer_function = indexer_function_with_matches.indexer_function;
            let indexer_rule_matches = indexer_function_with_matches.matches;

            for _ in indexer_rule_matches.iter() {
                tracing::debug!(
                    target: INDEXER,
                    "Matched filter {:?} for function {} {}",
                    indexer_function.indexer_rule.matching_rule,
                    indexer_function.account_id,
                    indexer_function.function_name,
                );

                if !indexer_function.provisioned {
                    set_provisioned_flag(context.indexer_registry, indexer_function).await;
                }

                storage::sadd(
                    context.redis_connection_manager,
                    storage::STREAMS_SET_KEY,
                    storage::generate_real_time_stream_key(&indexer_function.get_full_name()),
                )
                .await?;
                storage::set(
                    context.redis_connection_manager,
                    storage::generate_real_time_storage_key(&indexer_function.get_full_name()),
                    serde_json::to_string(indexer_function)?,
                    None,
                )
                .await?;
                storage::xadd(
                    context.redis_connection_manager,
                    storage::generate_real_time_stream_key(&indexer_function.get_full_name()),
                    &[("block_height", block_height)],
                )
                .await?;
            }
        }
    }

    // cache last indexed block height
    storage::update_last_indexed_block(
        context.redis_connection_manager,
        context.streamer_message.block.header.height,
    )
    .await?;

    metrics::BLOCK_COUNT.inc();
    metrics::LATEST_BLOCK_HEIGHT.set(
        context
            .streamer_message
            .block
            .header
            .height
            .try_into()
            .unwrap(),
    );

    Ok(context.streamer_message.block.header.height)
}

async fn set_provisioned_flag(
    indexer_registry: &SharedIndexerRegistry,
    indexer_function: &IndexerFunction,
) {
    match indexer_registry
        .lock()
        .await
        .get_mut(&indexer_function.account_id)
    {
        Some(account_functions) => {
            match account_functions.get_mut(&indexer_function.function_name) {
                Some(indexer_function) => {
                    indexer_function.provisioned = true;
                }
                None => {
                    let keys = account_functions
                        .keys()
                        .map(|s| &**s)
                        .collect::<Vec<_>>()
                        .join(", ");
                    tracing::error!(
                        target: INDEXER,
                        "Unable to set provisioning status, Indexer function with account_id {} and function_name {} not found in registry. Functions for this account are: {}",
                        indexer_function.account_id,
                        indexer_function.function_name,
                        keys
                    );
                }
            }
        }
        None => {
            tracing::error!(
                target: INDEXER,
                "Unable to set provisioning status, Indexer function account id '{}' not found in registry",
                indexer_function.account_id
            );
        }
    }
}

struct IndexerFunctionWithMatches<'b> {
    pub indexer_function: &'b IndexerFunction,
    pub matches: Vec<IndexerRuleMatch>,
}

async fn reduce_rule_matches_for_indexer_function<'x>(
    indexer_function: &'x IndexerFunction,
    streamer_message: &StreamerMessage,
    chain_id: ChainId,
) -> anyhow::Result<IndexerFunctionWithMatches<'x>> {
    let matches = indexer_rules_engine::reduce_indexer_rule_matches(
        &indexer_function.indexer_rule,
        streamer_message,
        chain_id.clone(),
    )
    .await?;
    Ok(IndexerFunctionWithMatches {
        indexer_function,
        matches,
    })
}

#[cfg(test)]
mod historical_block_processing_integration_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use indexer_rule_type::indexer_rule::{IndexerRule, IndexerRuleKind, MatchingRule, Status};
    use std::collections::HashMap;

    #[tokio::test]
    async fn set_provisioning_finds_functions_in_registry() {
        let mut indexer_registry = IndexerRegistry::new();
        let indexer_function = IndexerFunction {
            account_id: "test_near".to_string().parse().unwrap(),
            function_name: "test_indexer".to_string(),
            code: "".to_string(),
            start_block_height: None,
            schema: None,
            provisioned: false,
            indexer_rule: IndexerRule {
                indexer_rule_kind: IndexerRuleKind::Action,
                id: None,
                name: None,
                matching_rule: MatchingRule::ActionAny {
                    affected_account_id: "social.near".to_string(),
                    status: Status::Success,
                },
            },
        };

        let mut functions: HashMap<String, IndexerFunction> = HashMap::new();
        functions.insert(
            indexer_function.function_name.clone(),
            indexer_function.clone(),
        );
        indexer_registry.insert(indexer_function.account_id.clone(), functions);

        let indexer_registry: SharedIndexerRegistry =
            std::sync::Arc::new(Mutex::new(indexer_registry));
        let mut indexer_registry_locked = indexer_registry.lock().await;

        set_provisioned_flag(&indexer_registry, &indexer_function);

        let account_functions = indexer_registry_locked
            .get(&indexer_function.account_id)
            .unwrap();
        let indexer_function = account_functions
            .get(&indexer_function.function_name)
            .unwrap();

        assert!(indexer_function.provisioned);
    }
}
