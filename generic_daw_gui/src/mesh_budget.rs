use std::sync::atomic::{AtomicUsize, Ordering};

/// Maximum number of meshes to generate per frame.
const MESH_BUDGET_PER_FRAME: usize = 3;

/// Current remaining budget for this frame.
static BUDGET_REMAINING: AtomicUsize = AtomicUsize::new(MESH_BUDGET_PER_FRAME);

/// Global mesh generation budget per frame.
/// This limits how many waveform meshes can be generated in a single frame
/// to prevent frame spikes when many clips need meshes simultaneously.
pub struct MeshBudget;

impl MeshBudget {
	/// Try to consume one unit of budget.
	/// Returns true if budget was available, false if exhausted.
	#[must_use]
	pub fn try_consume() -> bool {
		BUDGET_REMAINING
			.fetch_update(Ordering::AcqRel, Ordering::Acquire, |remaining| {
				remaining.checked_sub(1)
			})
			.is_ok()
	}

	/// Reset the budget to full at the start of a new frame.
	pub fn reset() {
		BUDGET_REMAINING.store(MESH_BUDGET_PER_FRAME, Ordering::Release);
	}

	/// Check if any budget remains.
	#[must_use]
	pub fn has_budget() -> bool {
		BUDGET_REMAINING.load(Ordering::Acquire) > 0
	}
}
