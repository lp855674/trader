#[derive(Debug, Clone, PartialEq)]
pub enum IcebergState {
    Active,
    Replenishing,
    Filled,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct IcebergOrder {
    pub total_qty: f64,
    pub display_qty: f64,
    pub filled_qty: f64,
    pub active_slice_qty: f64,
    pub state: IcebergState,
}

impl IcebergOrder {
    pub fn new(total_qty: f64, display_qty: f64) -> Self {
        let active_slice = display_qty.min(total_qty);
        Self {
            total_qty,
            display_qty,
            filled_qty: 0.0,
            active_slice_qty: active_slice,
            state: IcebergState::Active,
        }
    }

    /// Apply a fill. Returns true if replenishment is needed.
    pub fn apply_fill(&mut self, fill_qty: f64) -> bool {
        self.filled_qty += fill_qty;
        self.active_slice_qty -= fill_qty;
        if self.active_slice_qty < 0.0 {
            self.active_slice_qty = 0.0;
        }
        if self.filled_qty >= self.total_qty {
            self.state = IcebergState::Filled;
            return false;
        }
        if self.active_slice_qty <= 0.0 {
            self.state = IcebergState::Replenishing;
            return true;
        }
        false
    }

    /// Refill the active slice from remaining quantity.
    pub fn replenish(&mut self) {
        if self.state == IcebergState::Replenishing {
            let remaining = self.remaining();
            self.active_slice_qty = self.display_qty.min(remaining);
            if self.active_slice_qty > 0.0 {
                self.state = IcebergState::Active;
            }
        }
    }

    pub fn remaining(&self) -> f64 {
        (self.total_qty - self.filled_qty).max(0.0)
    }

    pub fn is_complete(&self) -> bool {
        self.state == IcebergState::Filled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iceberg_replenishes_correctly() {
        let mut iceberg = IcebergOrder::new(10.0, 3.0);
        assert!((iceberg.active_slice_qty - 3.0).abs() < 1e-9);

        // Fill the first slice
        let needs_replenish = iceberg.apply_fill(3.0);
        assert!(needs_replenish);
        assert_eq!(iceberg.state, IcebergState::Replenishing);

        iceberg.replenish();
        assert_eq!(iceberg.state, IcebergState::Active);
        assert!((iceberg.active_slice_qty - 3.0).abs() < 1e-9);
    }

    #[test]
    fn iceberg_completes_when_fully_filled() {
        let mut iceberg = IcebergOrder::new(6.0, 3.0);
        iceberg.apply_fill(3.0);
        iceberg.replenish();
        let needs = iceberg.apply_fill(3.0);
        assert!(!needs);
        assert!(iceberg.is_complete());
        assert!((iceberg.remaining() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn iceberg_last_slice_smaller_than_display() {
        let mut iceberg = IcebergOrder::new(5.0, 3.0);
        iceberg.apply_fill(3.0);
        iceberg.replenish();
        // Remaining = 2, display = 3 → active slice = min(3, 2) = 2
        assert!((iceberg.active_slice_qty - 2.0).abs() < 1e-9);
    }
}
