use std::collections::HashMap;

const MS_PER_DAY: i64 = 86_400_000;

// ── TradingHours ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TradingHours {
    pub open_ms: u64,
    pub close_ms: u64,
    pub timezone: String,
}

impl TradingHours {
    pub fn is_trading(&self, ts_ms: i64) -> bool {
        let ms_of_day = ((ts_ms % MS_PER_DAY) + MS_PER_DAY) as u64 % MS_PER_DAY as u64;
        if self.open_ms <= self.close_ms {
            ms_of_day >= self.open_ms && ms_of_day < self.close_ms
        } else {
            // Overnight session
            ms_of_day >= self.open_ms || ms_of_day < self.close_ms
        }
    }

    pub fn next_open(&self, ts_ms: i64) -> i64 {
        let ms_of_day = ((ts_ms % MS_PER_DAY) + MS_PER_DAY) as u64 % MS_PER_DAY as u64;
        let day_start = ts_ms - ms_of_day as i64;
        if ms_of_day < self.open_ms {
            day_start + self.open_ms as i64
        } else {
            day_start + MS_PER_DAY + self.open_ms as i64
        }
    }
}

// ── HolidayCalendar ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct HolidayCalendar {
    pub holidays: Vec<i64>,
}

impl HolidayCalendar {
    pub fn is_holiday(&self, ts_ms: i64) -> bool {
        let day = (ts_ms as f64 / MS_PER_DAY as f64).floor() as i64;
        self.holidays.iter().any(|h| {
            let h_day = (*h as f64 / MS_PER_DAY as f64).floor() as i64;
            h_day == day
        })
    }

    pub fn next_trading_day(&self, ts_ms: i64, hours: &TradingHours) -> i64 {
        let mut candidate = hours.next_open(ts_ms);
        for _ in 0..365 {
            if !self.is_holiday(candidate) {
                return candidate;
            }
            candidate += MS_PER_DAY;
        }
        candidate
    }
}

// ── InstrumentMetadata ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct InstrumentMetadata {
    pub symbol: String,
    pub exchange: String,
    pub currency: String,
    pub tick_size: f64,
    pub lot_size: f64,
    pub hours: TradingHours,
    pub calendar: HolidayCalendar,
    pub listing_date_ms: Option<i64>,
    pub delisting_date_ms: Option<i64>,
}

impl InstrumentMetadata {
    pub fn is_active(&self, ts_ms: i64) -> bool {
        if let Some(listing) = self.listing_date_ms {
            if ts_ms < listing {
                return false;
            }
        }
        if let Some(delisting) = self.delisting_date_ms {
            if ts_ms >= delisting {
                return false;
            }
        }
        true
    }
}

// ── MetadataManager ───────────────────────────────────────────────────────────

#[derive(Default)]
pub struct MetadataManager {
    pub instruments: HashMap<String, InstrumentMetadata>,
}

impl MetadataManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, symbol: String, meta: InstrumentMetadata) {
        self.instruments.insert(symbol, meta);
    }

    pub fn get(&self, symbol: &str) -> Option<&InstrumentMetadata> {
        self.instruments.get(symbol)
    }

    pub fn active_instruments(&self, ts_ms: i64) -> Vec<&InstrumentMetadata> {
        self.instruments
            .values()
            .filter(|m| m.is_active(ts_ms))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_hours() -> TradingHours {
        TradingHours {
            open_ms: 9 * 3_600_000,   // 09:00 UTC
            close_ms: 17 * 3_600_000, // 17:00 UTC
            timezone: "UTC".to_string(),
        }
    }

    fn make_meta(symbol: &str, listing: Option<i64>, delisting: Option<i64>) -> InstrumentMetadata {
        InstrumentMetadata {
            symbol: symbol.to_string(),
            exchange: "TEST".to_string(),
            currency: "USD".to_string(),
            tick_size: 0.01,
            lot_size: 1.0,
            hours: default_hours(),
            calendar: HolidayCalendar::default(),
            listing_date_ms: listing,
            delisting_date_ms: delisting,
        }
    }

    #[test]
    fn trading_hours_check() {
        let hours = default_hours();
        // 10:00 UTC = 10 * 3600000 ms since midnight
        let ts_10am = 10 * 3_600_000_i64;
        assert!(hours.is_trading(ts_10am));
        // 20:00 UTC = 20 * 3600000 ms since midnight
        let ts_8pm = 20 * 3_600_000_i64;
        assert!(!hours.is_trading(ts_8pm));
    }

    #[test]
    fn holiday_is_detected() {
        let calendar = HolidayCalendar {
            holidays: vec![0], // 1970-01-01
        };
        assert!(calendar.is_holiday(0));
        assert!(calendar.is_holiday(3_600_000)); // still same day
        assert!(!calendar.is_holiday(MS_PER_DAY)); // next day
    }

    #[test]
    fn active_instruments_filter() {
        let mut mgr = MetadataManager::new();
        mgr.register("ACTIVE".to_string(), make_meta("ACTIVE", Some(0), None));
        mgr.register(
            "FUTURE".to_string(),
            make_meta("FUTURE", Some(9_999_999_999), None),
        );
        mgr.register(
            "DELISTED".to_string(),
            make_meta("DELISTED", Some(0), Some(1000)),
        );

        let active = mgr.active_instruments(5000);
        let symbols: Vec<&str> = active.iter().map(|m| m.symbol.as_str()).collect();
        assert!(symbols.contains(&"ACTIVE"));
        assert!(!symbols.contains(&"FUTURE"));
        assert!(!symbols.contains(&"DELISTED"));
    }
}
