use rapids_engine::{ScaleDecision, scaling_decision, tile};

fn assert_exact_cover(total: u64, min_segment_bytes: u64, max_segments: u32) {
    let segments = tile(total, min_segment_bytes, max_segments);
    if total == 0 {
        assert!(segments.is_empty());
        return;
    }
    assert!(!segments.is_empty());
    assert!(segments.len() <= max_segments as usize);
    let mut cursor = 0u64;
    for (position, segment) in segments.iter().enumerate() {
        assert_eq!(segment.index as usize, position);
        assert_eq!(segment.start, cursor);
        assert!(segment.end >= segment.start);
        assert!(segment.size() >= 1);
        cursor = segment.end + 1;
    }
    assert_eq!(cursor, total, "tiles must cover the file exactly");
}

#[test]
fn tiles_cover_files_exactly() {
    let cases: [(u64, u64, u32); 12] = [
        (1, 1, 1),
        (2, 1, 32),
        (31, 1, 32),
        (100, 7, 32),
        (1024 * 1024 - 1, 1024, 32),
        (1024 * 1024, 1024, 32),
        (8 * 1024 * 1024, 1024 * 1024, 32),
        (40 * 1024 * 1024, 1024 * 1024, 32),
        (100 * 1024 * 1024, 1024 * 1024, 32),
        (100 * 1024 * 1024 + 7, 1024 * 1024, 32),
        (999, 3, 3),
        (5_000_000_000, 1024 * 1024, 32),
    ];
    for (total, min, max) in cases {
        assert_exact_cover(total, min, max);
    }
}

#[test]
fn respects_segment_count_rules() {
    assert_eq!(tile(0, 1024 * 1024, 32).len(), 0);
    assert_eq!(tile(3, 1, 32).len(), 3);
    assert_eq!(tile(8 * 1024 * 1024, 1024 * 1024, 32).len(), 8);
    assert_eq!(tile(40 * 1024 * 1024, 1024 * 1024, 32).len(), 32);
    assert_eq!(tile(u64::MAX, 1024 * 1024, 32).len(), 32);
    assert_eq!(
        tile(100, 0, 0).len(),
        1,
        "zero limits still produce one exact tile"
    );
}

#[test]
fn scaling_requires_measured_gain() {
    assert_eq!(scaling_decision(100.0, 125.0, 0.20), ScaleDecision::Add);
    assert_eq!(scaling_decision(100.0, 119.0, 0.20), ScaleDecision::Settle);
    assert_eq!(scaling_decision(100.0, 100.0, 0.20), ScaleDecision::Settle);
    assert_eq!(scaling_decision(100.0, 50.0, 0.20), ScaleDecision::Settle);
    assert_eq!(scaling_decision(0.0, 0.0, 0.20), ScaleDecision::Settle);
    assert_eq!(scaling_decision(0.0, 50.0, 0.20), ScaleDecision::Add);
    assert_eq!(
        scaling_decision(f64::NAN, 10.0, 0.20),
        ScaleDecision::Settle
    );
}
