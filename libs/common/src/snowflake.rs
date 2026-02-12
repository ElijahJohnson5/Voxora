use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Custom epoch: 2025-01-01T00:00:00Z in milliseconds since Unix epoch.
const VOXORA_EPOCH_MS: u64 = 1_735_689_600_000;

const WORKER_BITS: u64 = 10;
const SEQUENCE_BITS: u64 = 12;
const SEQUENCE_MASK: u64 = (1 << SEQUENCE_BITS) - 1; // 4095

struct State {
    last_ms: u64,
    sequence: u64,
}

/// Discord-style 64-bit snowflake ID generator.
///
/// Layout (MSB → LSB):
/// - Bits 63–22: Timestamp (42 bits) — ms since Voxora epoch
/// - Bits 21–12: Worker ID (10 bits)
/// - Bits 11–0:  Sequence (12 bits) — per-ms counter, max 4096/ms
pub struct SnowflakeGenerator {
    worker_id: u64,
    state: Mutex<State>,
}

impl SnowflakeGenerator {
    pub fn new(worker_id: u16) -> Self {
        assert!(
            (worker_id as u64) < (1 << WORKER_BITS),
            "worker_id must fit in {WORKER_BITS} bits"
        );
        Self {
            worker_id: worker_id as u64,
            state: Mutex::new(State {
                last_ms: 0,
                sequence: 0,
            }),
        }
    }

    pub fn generate(&self) -> i64 {
        let mut state = self.state.lock().unwrap();

        let mut now_ms = current_ms();

        if now_ms < state.last_ms {
            panic!(
                "Clock moved backwards: last_ms={}, now_ms={}",
                state.last_ms, now_ms
            );
        }

        if now_ms == state.last_ms {
            state.sequence = (state.sequence + 1) & SEQUENCE_MASK;
            if state.sequence == 0 {
                // Sequence exhausted for this millisecond — spin-wait.
                while now_ms == state.last_ms {
                    now_ms = current_ms();
                }
            }
        } else {
            state.sequence = 0;
        }

        state.last_ms = now_ms;

        let ts = now_ms - VOXORA_EPOCH_MS;
        let id = (ts << (WORKER_BITS + SEQUENCE_BITS))
            | (self.worker_id << SEQUENCE_BITS)
            | state.sequence;

        id as i64
    }
}

fn current_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before Unix epoch")
        .as_millis() as u64
}

/// Extract the creation timestamp (ms since Unix epoch) from a snowflake ID.
pub fn snowflake_timestamp_ms(id: i64) -> u64 {
    let ts = (id as u64) >> (WORKER_BITS + SEQUENCE_BITS);
    ts + VOXORA_EPOCH_MS
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn generates_unique_ids() {
        let gen = SnowflakeGenerator::new(0);
        let mut ids = HashSet::new();
        for _ in 0..10_000 {
            let id = gen.generate();
            assert!(ids.insert(id), "duplicate snowflake: {id}");
        }
    }

    #[test]
    fn ids_are_monotonically_increasing() {
        let gen = SnowflakeGenerator::new(0);
        let mut prev = 0i64;
        for _ in 0..1_000 {
            let id = gen.generate();
            assert!(id > prev, "not monotonic: {prev} >= {id}");
            prev = id;
        }
    }

    #[test]
    fn timestamp_extraction_round_trips() {
        let gen = SnowflakeGenerator::new(0);
        let before = current_ms();
        let id = gen.generate();
        let after = current_ms();

        let extracted = snowflake_timestamp_ms(id);
        assert!(
            extracted >= before && extracted <= after,
            "extracted={extracted}, before={before}, after={after}"
        );
    }

    #[test]
    fn ids_are_positive() {
        let gen = SnowflakeGenerator::new(0);
        for _ in 0..100 {
            assert!(gen.generate() > 0);
        }
    }
}
