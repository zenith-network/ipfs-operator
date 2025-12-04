use std::collections::BTreeMap;

pub mod bootstrap;
pub mod storage;

#[derive(Debug, Clone)]
pub enum AnnounceAddr {
    StandardCluster(String),
    ArchiveCluster(BTreeMap<String, String>),
}
