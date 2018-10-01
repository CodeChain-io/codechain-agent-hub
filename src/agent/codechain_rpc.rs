use std::net::SocketAddr;

use jsonrpc_core::types::{Failure, Output, Success};
use serde::de::DeserializeOwned;
use serde_json;

use super::super::common_rpc_types::{BlockId, NodeStatus};
use super::agent::{AgentSender, SendAgentRPC};
use super::types::ChainGetBestBlockIdResponse;

pub struct CodeChainRPC {
    sender: AgentSender,
}

impl CodeChainRPC {
    pub fn new(sender: AgentSender) -> Self {
        Self {
            sender,
        }
    }

    pub fn get_peers(&self, status: NodeStatus) -> Result<Vec<SocketAddr>, String> {
        self.call_rpc(status, "net_getEstablishedPeers")
    }

    pub fn get_best_block_id(&self, status: NodeStatus) -> Result<Option<BlockId>, String> {
        let response: Option<ChainGetBestBlockIdResponse> = self.call_rpc(status, "chain_getBestBlockId")?;

        Ok(response.map(|response| BlockId {
            block_number: response.number,
            hash: response.hash,
        }))
    }

    pub fn version(&self, status: NodeStatus) -> Result<Option<String>, String> {
        self.call_rpc(status, "version")
    }

    pub fn commit_hash(&self, status: NodeStatus) -> Result<Option<String>, String> {
        self.call_rpc(status, "commitHash")
    }

    fn call_rpc<T>(&self, status: NodeStatus, method: &str) -> Result<T, String>
    where
        T: Default + DeserializeOwned, {
        if status != NodeStatus::Run {
            return Ok(Default::default())
        }

        let response =
            self.sender.codechain_call_rpc((method.to_string(), Vec::new())).map_err(|err| format!("{}", err))?;

        let response: T = match response {
            Output::Success(Success {
                result,
                ..
            }) => serde_json::from_value(result).map_err(|err| format!("{}", err))?,
            Output::Failure(Failure {
                error,
                ..
            }) => return Err(format!("get_peers error {:#?}", error)),
        };

        Ok(response)
    }
}
