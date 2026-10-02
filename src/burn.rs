//CPU and memory burn for one request, run on a blocking thread.

use std::hint::black_box;
use std::time::{Duration, Instant};

const PAGE: usize = 4096;
const MIB: usize = 1024 * 1024;

pub fn burn(cpu_ms: u64, mem_mib: u64) -> u64 {
    let started = Instant::now();
    let budget = Duration::from_millis(cpu_ms);
    let size = usize::try_from(mem_mib).map_or(usize::MAX, |mib| mib.saturating_mul(MIB));
    let mut buffer = vec![0u8; size];
    for byte in buffer.iter_mut().step_by(PAGE) {
        *byte = 1;
    }
    black_box(&mut buffer);
    while started.elapsed() < budget {
        for byte in buffer.iter_mut().step_by(PAGE) {
            *byte = black_box(byte.wrapping_add(1));
        }
        black_box(&mut buffer);
    }
    drop(buffer);
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}
