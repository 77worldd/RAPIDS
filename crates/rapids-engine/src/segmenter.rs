/// Splitting a file into byte ranges and deciding when another connection
/// is worth opening.

/// An inclusive byte range inside the destination file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub index: u32,
    pub start: u64,
    /// Inclusive end offset.
    pub end: u64,
}

impl Segment {
    /// Number of bytes covered by this segment.
    pub fn size(&self) -> u64 {
        self.end - self.start + 1
    }
}

/// Tiles `[0, total)` into at most `max_segments` ranges, each at least
/// `min_segment_bytes` when the file allows it. The tiles always cover the
/// file exactly: no gaps, no overlaps, integer-safe edges.
pub fn tile(total: u64, min_segment_bytes: u64, max_segments: u32) -> Vec<Segment> {
    if total == 0 {
        return Vec::new();
    }
    let min_segment_bytes = min_segment_bytes.max(1);
    let by_size = total.div_ceil(min_segment_bytes);
    let count = by_size.min(max_segments as u64).max(1);
    let base = total / count;
    let remainder = total % count;

    let mut segments = Vec::with_capacity(count as usize);
    let mut start = 0u64;
    for index in 0..count {
        let extra = if index < remainder { 1 } else { 0 };
        let size = base + extra;
        segments.push(Segment {
            index: index as u32,
            start,
            end: start + size - 1,
        });
        start += size;
    }
    segments
}

/// Outcome of comparing two throughput measurement windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScaleDecision {
    /// The latest connection paid for itself: open another one.
    Add,
    /// The gain fell below the threshold (or there was no progress): settle.
    Settle,
}

/// Compares throughput of `k` connections against the previous `k-1`
/// window. Scaling stops as soon as the relative gain drops below
/// `gain_threshold`.
pub fn scaling_decision(previous_bps: f64, current_bps: f64, gain_threshold: f64) -> ScaleDecision {
    if !previous_bps.is_finite() || !current_bps.is_finite() || previous_bps < 0.0 {
        return ScaleDecision::Settle;
    }
    if previous_bps == 0.0 {
        return if current_bps > 0.0 {
            ScaleDecision::Add
        } else {
            ScaleDecision::Settle
        };
    }
    let gain = (current_bps - previous_bps) / previous_bps;
    if gain >= gain_threshold {
        ScaleDecision::Add
    } else {
        ScaleDecision::Settle
    }
}
