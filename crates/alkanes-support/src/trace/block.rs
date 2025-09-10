use crate::trace::types::TraceEvent;
use bitcoin::OutPoint;

#[derive(Clone, Debug, Default)]
pub struct BlockTraceItem {
    pub outpoint: OutPoint,
    pub trace: Vec<TraceEvent>,
}
