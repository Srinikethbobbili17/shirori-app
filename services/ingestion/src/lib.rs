//! Shirori ingestion worker (architecture §5.2 "Ingest scheduler / parser", build step 3).
//!
//! Polls approved, rights-cleared feeds and puts their items in the editors' review queue.
//! Nothing reaches readers from here: an editor promotes an item to a source work, writes a
//! card, and the card still has to pass the publication gate.

pub mod canonical;
pub mod config;
pub mod net;
pub mod parse;
pub mod poll;
pub mod robots;
pub mod store;
pub mod worker;
