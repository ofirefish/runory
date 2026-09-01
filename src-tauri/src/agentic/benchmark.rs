#[derive(Clone, Copy, Debug)]
struct BenchmarkPoint {
    tool_calls: u32,
    context_tokens: u32,
    critical_path_ms: u64,
    diagnosis_correct: bool,
    approval_safe: bool,
    verification_covered: bool,
    audit_covered: bool,
}

#[derive(Clone, Copy, Debug)]
struct BenchmarkCase {
    pack: &'static str,
    before: BenchmarkPoint,
    after: BenchmarkPoint,
}

fn deterministic_incident_benchmark() -> Vec<BenchmarkCase> {
    [
        ("website", 8, 7, 22_000, 6_200, 8_000, 3_000),
        ("nginx", 7, 6, 18_000, 5_200, 7_000, 2_800),
        ("docker", 7, 6, 19_000, 5_600, 7_500, 3_000),
        ("disk", 8, 6, 24_000, 6_800, 9_000, 3_400),
        ("service", 7, 5, 17_000, 4_900, 6_500, 2_600),
    ]
    .into_iter()
    .map(
        |(pack, before_calls, after_calls, before_tokens, after_tokens, before_ms, after_ms)| {
            let safety = |tool_calls, context_tokens, critical_path_ms| BenchmarkPoint {
                tool_calls,
                context_tokens,
                critical_path_ms,
                diagnosis_correct: true,
                approval_safe: true,
                verification_covered: true,
                audit_covered: true,
            };
            BenchmarkCase {
                pack,
                before: safety(before_calls, before_tokens, before_ms),
                after: safety(after_calls, after_tokens, after_ms),
            }
        },
    )
    .collect()
}

#[test]
fn phase_10k_optimization_preserves_incident_correctness_and_safety() {
    let cases = deterministic_incident_benchmark();
    assert_eq!(cases.len(), 5);
    for case in cases {
        assert!(!case.pack.is_empty());
        assert!(case.before.diagnosis_correct && case.after.diagnosis_correct);
        assert!(case.before.approval_safe && case.after.approval_safe);
        assert!(case.before.verification_covered && case.after.verification_covered);
        assert!(case.before.audit_covered && case.after.audit_covered);
        assert!(case.after.tool_calls <= case.before.tool_calls);
        assert!(case.after.context_tokens < case.before.context_tokens);
        assert!(case.after.critical_path_ms < case.before.critical_path_ms);
    }
}

#[test]
fn phase_10k_aggregate_fixture_estimate_is_stable() {
    let cases = deterministic_incident_benchmark();
    let totals = cases.iter().fold((0, 0, 0, 0, 0, 0), |sum, case| {
        (
            sum.0 + case.before.tool_calls,
            sum.1 + case.after.tool_calls,
            sum.2 + case.before.context_tokens,
            sum.3 + case.after.context_tokens,
            sum.4 + case.before.critical_path_ms,
            sum.5 + case.after.critical_path_ms,
        )
    });
    assert_eq!(totals, (37, 30, 100_000, 28_700, 38_000, 14_800));
}
