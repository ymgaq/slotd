use crate::app::error::Result;
use crate::model::job::NodeInfo;

use super::Store;

impl Store {
    pub fn node_info(&self) -> Result<NodeInfo> {
        let mut partitions = Vec::new();
        for partition in self.config.active_partitions() {
            partitions.push(self.partition_info(&partition)?);
        }
        Ok(NodeInfo { partitions })
    }
}
