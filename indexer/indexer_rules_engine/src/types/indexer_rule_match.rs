use std::fmt;

pub type TransactionHashString = String;
pub type ReceiptIdString = String;
pub type BlockHashString = String;

#[derive(
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Debug,
)]
pub struct IndexerRuleMatch {
    pub chain_id: ChainId,
    pub indexer_rule_id: Option<u32>,
    pub indexer_rule_name: Option<String>,
    pub payload: IndexerRuleMatchPayload,
    pub block_height: u64,
}

impl IndexerRuleMatch {
    pub fn explorer_link(&self) -> String {
        match self.chain_id {
            ChainId::Testnet => {
                if let Some(tx_hash) = self.payload.transaction_hash() {
                    if let Some(receipt_id) = self.payload.receipt_id() {
                        format!(
                            "https://explorer.testnet.near.org/transactions/{}#{}",
                            tx_hash, receipt_id,
                        )
                    } else {
                        format!("https://explorer.testnet.near.org/transactions/{}", tx_hash)
                    }
                } else {
                    format!(
                        "https://explorer.testnet.near.org/block/{}",
                        self.payload.block_hash()
                    )
                }
            }
            ChainId::Mainnet => {
                if let Some(tx_hash) = self.payload.transaction_hash() {
                    if let Some(receipt_id) = self.payload.receipt_id() {
                        format!(
                            "https://explorer.near.org/transactions/{}#{}",
                            tx_hash, receipt_id,
                        )
                    } else {
                        format!("https://explorer.near.org/transactions/{}", tx_hash)
                    }
                } else {
                    format!(
                        "https://explorer.near.org/block/{}",
                        self.payload.block_hash()
                    )
                }
            }
        }
    }
}

#[derive(
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Debug,
)]
pub enum IndexerRuleMatchPayload {
    Actions {
        block_hash: BlockHashString,
        receipt_id: ReceiptIdString,
        transaction_hash: Option<TransactionHashString>,
    },
    Events {
        block_hash: BlockHashString,
        receipt_id: ReceiptIdString,
        transaction_hash: Option<TransactionHashString>,
        event: String,
        standard: String,
        version: String,
        data: Option<String>,
    },
    StateChanges {
        block_hash: BlockHashString,
        receipt_id: Option<ReceiptIdString>,
        transaction_hash: Option<TransactionHashString>,
    },
}

impl IndexerRuleMatchPayload {
    pub fn block_hash(&self) -> BlockHashString {
        match self {
            Self::Actions { block_hash, .. }
            | Self::Events { block_hash, .. }
            | Self::StateChanges { block_hash, .. } => block_hash.to_string(),
        }
    }

    pub fn receipt_id(&self) -> Option<ReceiptIdString> {
        match self {
            Self::Actions { receipt_id, .. } | Self::Events { receipt_id, .. } => {
                Some(receipt_id.to_string())
            }
            Self::StateChanges { receipt_id, .. } => receipt_id.clone(),
        }
    }

    pub fn transaction_hash(&self) -> Option<TransactionHashString> {
        match self {
            Self::Actions {
                transaction_hash, ..
            }
            | Self::Events {
                transaction_hash, ..
            }
            | Self::StateChanges {
                transaction_hash, ..
            } => transaction_hash.clone(),
        }
    }
}

#[derive(
    borsh::BorshSerialize,
    borsh::BorshDeserialize,
    serde::Serialize,
    serde::Deserialize,
    Clone,
    Debug,
)]
pub enum ChainId {
    Mainnet,
    Testnet,
}
impl fmt::Display for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ChainId::Mainnet => write!(f, "mainnet"),
            ChainId::Testnet => write!(f, "testnet"),
        }
    }
}
