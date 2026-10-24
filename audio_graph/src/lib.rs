use utils::unique_id;

mod audio_graph;
mod entry;
mod event_impl;
mod node_impl;
#[cfg(feature = "simd")]
pub mod simd;

unique_id!(NodeId);

pub use audio_graph::AudioGraph;
pub use event_impl::EventImpl;
pub use node_impl::{Injector, NodeImpl, ThreadPool};
pub use thread_pool;
