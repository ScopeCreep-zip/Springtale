//! Phase C — drift detection over the executions log.
//!
//! Reads recent rows from `executions` (+ `execution_steps` for
//! per-kind step stats) and surfaces three trend signals:
//!
//! 1. **Latency drift** — median + p95 duration_ms vs. baseline.
//! 2. **Success-rate drift** — share of `succeeded` rows over the
//!    window vs. baseline.
//! 3. **Refusal-rate drift** — share of `extract` steps whose
//!    `error_kind = "schema_invalid"` (the executions-log tag for
//!    LLM refusals / output-invalid). Catches a model getting
//!    stricter / a schema getting brittle.
//!
//! The "drift" is the delta between the most-recent `recent_n`
//! runs and the older `baseline_n` runs that preceded them.
//! Classification: steady / improving / degrading per signal,
//! resolved server-side per `feedback_zero_frontend_logic`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use specta::Type;
use springtale_store::backend::StorageBackend;
use springtale_store::schema::executions::{ExecutionFilter, ExecutionStatus, ExecutionSummary};

use crate::error::OperationError;

/// Default sliding-window sizes — recent vs. baseline.
const DEFAULT_RECENT_N: u32 = 10;
const DEFAULT_BASELINE_N: u32 = 30;
/// Minimum total runs before a delta is reportable. Below this
/// the classification is always `NotEnoughData` (the frontend hides
/// the badge — no point alarming users on noise).
const MIN_TOTAL_RUNS_FOR_DELTA: usize = 5;

/// Input shape for [`recipe_drift`] / [`rule_drift`].
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct DriftFilter {
    /// Filter to one bot's runs. Both this and `formation_id`
    /// can be set — they intersect.
    pub bot_id: Option<String>,
    /// Filter to one formation's runs.
    pub formation_id: Option<String>,
    /// Filter to one rule's runs. Required for `rule_drift`.
    pub rule_id: Option<String>,
    /// Most-recent run count. Default 10.
    pub recent_n: Option<u32>,
    /// Older runs preceding `recent_n`. Default 30.
    pub baseline_n: Option<u32>,
}

/// Aggregate drift signal for a recipe / rule.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct DriftReport {
    /// How many recent runs informed the analysis.
    pub recent_runs: u32,
    /// How many baseline runs informed the analysis.
    pub baseline_runs: u32,
    /// Latency drift — median + p95 duration_ms delta.
    pub latency: LatencyDrift,
    /// Success-rate drift (fraction of `succeeded` rows).
    pub success_rate: RateDrift,
    /// Refusal-rate drift (fraction of `extract` steps with
    /// `schema_invalid` / `refused` errors).
    pub refusal_rate: RateDrift,
    /// Overall verdict — the worst-of the three signals.
    pub overall: DriftClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct LatencyDrift {
    pub recent_median_ms: Option<i64>,
    pub recent_p95_ms: Option<i64>,
    pub baseline_median_ms: Option<i64>,
    pub baseline_p95_ms: Option<i64>,
    /// `recent_median - baseline_median`. Negative = recent faster.
    pub median_delta_ms: Option<i64>,
    pub class: DriftClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RateDrift {
    /// 0.0..=1.0 over the recent window.
    pub recent: Option<f64>,
    /// 0.0..=1.0 over the baseline window.
    pub baseline: Option<f64>,
    /// `recent - baseline`. Negative for success_rate = worse;
    /// positive for refusal_rate = worse. The `class` field
    /// resolves direction so consumers don't have to.
    pub delta: Option<f64>,
    pub class: DriftClass,
}

/// Per-signal verdict. Each signal can independently land on any
/// of these; the aggregate `DriftReport.overall` is the worst of
/// the three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DriftClass {
    /// Not enough runs to compute a delta (< MIN_TOTAL_RUNS_FOR_DELTA).
    NotEnoughData,
    /// Recent window matches baseline within tolerance.
    Steady,
    /// Recent window better than baseline (faster / higher success
    /// rate / lower refusal rate).
    Improving,
    /// Recent window worse than baseline (slower / lower success
    /// rate / higher refusal rate).
    Degrading,
}

impl DriftClass {
    /// Worst-of aggregation. Used to fold per-signal classes into
    /// the overall verdict.
    fn worse_of(a: DriftClass, b: DriftClass) -> DriftClass {
        use DriftClass::*;
        match (a, b) {
            (Degrading, _) | (_, Degrading) => Degrading,
            (NotEnoughData, x) | (x, NotEnoughData) => x,
            (Steady, _) | (_, Steady) => Steady,
            (Improving, Improving) => Improving,
        }
    }
}

/// Compute drift for a rule. The bot / formation filters intersect.
pub async fn rule_drift(
    store: &Arc<dyn StorageBackend>,
    filter: DriftFilter,
) -> Result<DriftReport, OperationError> {
    let recent_n = filter.recent_n.unwrap_or(DEFAULT_RECENT_N);
    let baseline_n = filter.baseline_n.unwrap_or(DEFAULT_BASELINE_N);

    let total = recent_n.saturating_add(baseline_n);
    let exec_filter = ExecutionFilter {
        bot_id: filter.bot_id.clone(),
        formation_id: filter.formation_id.clone(),
        rule_id: filter.rule_id.clone(),
        status: None,
        before: None,
        limit: Some(total),
    };
    let runs = store
        .list_executions(exec_filter)
        .await
        .map_err(OperationError::Store)?;

    let (recent, baseline) = split_at(&runs, recent_n as usize);
    let refusal = compute_refusal_drift(store, recent, baseline).await?;
    Ok(build_report(recent, baseline, refusal))
}

/// Compute drift for a recipe — convenience over [`rule_drift`]
/// when the caller has the recipe id but not the rule id (multiple
/// rules per recipe are merged). Today recipes carry one rule, so
/// this is identical shape; the API stays separate so it can grow
/// in Phase C+.
pub async fn recipe_drift(
    store: &Arc<dyn StorageBackend>,
    recipe_id: &str,
    mut filter: DriftFilter,
) -> Result<DriftReport, OperationError> {
    // Recipe rows aren't directly filterable in the executions
    // schema, but executions.recipe_id is recorded. Resolve via a
    // wider query + in-memory filter so the drift window is per
    // recipe, not per arbitrary rule.
    let recent_n = filter.recent_n.unwrap_or(DEFAULT_RECENT_N);
    let baseline_n = filter.baseline_n.unwrap_or(DEFAULT_BASELINE_N);
    // Over-fetch — we filter recipe_id in-memory below.
    let total = recent_n.saturating_add(baseline_n).saturating_mul(2);
    filter.rule_id = None;
    let exec_filter = ExecutionFilter {
        bot_id: filter.bot_id.clone(),
        formation_id: filter.formation_id.clone(),
        rule_id: None,
        status: None,
        before: None,
        limit: Some(total),
    };
    let mut runs = store
        .list_executions(exec_filter)
        .await
        .map_err(OperationError::Store)?;
    runs.retain(|r| r.recipe_id.as_deref() == Some(recipe_id));
    let total_kept = runs.len();
    if total_kept > (recent_n + baseline_n) as usize {
        runs.truncate((recent_n + baseline_n) as usize);
    }

    let (recent, baseline) = split_at(&runs, recent_n as usize);
    let refusal = compute_refusal_drift(store, recent, baseline).await?;
    Ok(build_report(recent, baseline, refusal))
}

fn split_at(
    runs: &[ExecutionSummary],
    recent_n: usize,
) -> (&[ExecutionSummary], &[ExecutionSummary]) {
    let split = runs.len().min(recent_n);
    let (recent, baseline) = runs.split_at(split);
    (recent, baseline)
}

fn build_report(
    recent: &[ExecutionSummary],
    baseline: &[ExecutionSummary],
    refusal: RateDrift,
) -> DriftReport {
    let latency = compute_latency_drift(recent, baseline);
    let success = compute_success_drift(recent, baseline);
    let overall = DriftClass::worse_of(
        DriftClass::worse_of(latency.class, success.class),
        refusal.class,
    );
    DriftReport {
        recent_runs: recent.len() as u32,
        baseline_runs: baseline.len() as u32,
        latency,
        success_rate: success,
        refusal_rate: refusal,
        overall,
    }
}

fn compute_latency_drift(
    recent: &[ExecutionSummary],
    baseline: &[ExecutionSummary],
) -> LatencyDrift {
    let recent_ms: Vec<i64> = recent.iter().filter_map(|r| r.duration_ms).collect();
    let baseline_ms: Vec<i64> = baseline.iter().filter_map(|r| r.duration_ms).collect();
    let recent_median = median(&recent_ms);
    let recent_p95 = percentile(&recent_ms, 95);
    let baseline_median = median(&baseline_ms);
    let baseline_p95 = percentile(&baseline_ms, 95);
    let delta = match (recent_median, baseline_median) {
        (Some(r), Some(b)) => Some(r - b),
        _ => None,
    };
    let class = classify_latency(recent.len() + baseline.len(), delta, baseline_median);
    LatencyDrift {
        recent_median_ms: recent_median,
        recent_p95_ms: recent_p95,
        baseline_median_ms: baseline_median,
        baseline_p95_ms: baseline_p95,
        median_delta_ms: delta,
        class,
    }
}

fn compute_success_drift(recent: &[ExecutionSummary], baseline: &[ExecutionSummary]) -> RateDrift {
    let recent_rate = success_rate(recent);
    let baseline_rate = success_rate(baseline);
    let delta = match (recent_rate, baseline_rate) {
        (Some(r), Some(b)) => Some(r - b),
        _ => None,
    };
    let class = classify_success(recent.len() + baseline.len(), delta);
    RateDrift {
        recent: recent_rate,
        baseline: baseline_rate,
        delta,
        class,
    }
}

async fn compute_refusal_drift(
    store: &Arc<dyn StorageBackend>,
    recent: &[ExecutionSummary],
    baseline: &[ExecutionSummary],
) -> Result<RateDrift, OperationError> {
    let recent_rate = refusal_rate_for(store, recent).await?;
    let baseline_rate = refusal_rate_for(store, baseline).await?;
    let delta = match (recent_rate, baseline_rate) {
        (Some(r), Some(b)) => Some(r - b),
        _ => None,
    };
    let class = classify_refusal(recent.len() + baseline.len(), delta);
    Ok(RateDrift {
        recent: recent_rate,
        baseline: baseline_rate,
        delta,
        class,
    })
}

async fn refusal_rate_for(
    store: &Arc<dyn StorageBackend>,
    runs: &[ExecutionSummary],
) -> Result<Option<f64>, OperationError> {
    if runs.is_empty() {
        return Ok(None);
    }
    let mut extract_steps = 0u32;
    let mut refusal_steps = 0u32;
    for run in runs {
        let steps = store
            .get_execution_steps(&run.id)
            .await
            .map_err(OperationError::Store)?;
        for step in steps {
            if step.step_kind != "extract" {
                continue;
            }
            extract_steps += 1;
            if matches!(
                step.error_kind.as_deref(),
                Some("schema_invalid" | "refused")
            ) {
                refusal_steps += 1;
            }
        }
    }
    if extract_steps == 0 {
        return Ok(None);
    }
    Ok(Some(refusal_steps as f64 / extract_steps as f64))
}

fn success_rate(runs: &[ExecutionSummary]) -> Option<f64> {
    if runs.is_empty() {
        return None;
    }
    let total = runs.len() as f64;
    let succeeded = runs
        .iter()
        .filter(|r| matches!(r.status, ExecutionStatus::Succeeded))
        .count() as f64;
    Some(succeeded / total)
}

fn median(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        Some((sorted[mid - 1] + sorted[mid]) / 2)
    } else {
        Some(sorted[mid])
    }
}

fn percentile(values: &[i64], p: u8) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = ((p as f64 / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    Some(sorted[rank.min(sorted.len() - 1)])
}

fn classify_latency(total: usize, delta: Option<i64>, baseline_median: Option<i64>) -> DriftClass {
    if total < MIN_TOTAL_RUNS_FOR_DELTA {
        return DriftClass::NotEnoughData;
    }
    let (Some(delta), Some(baseline)) = (delta, baseline_median) else {
        return DriftClass::NotEnoughData;
    };
    // Tolerance: ignore swings smaller than 25% of the baseline.
    // Below the floor (200ms) ignore swings smaller than 50ms.
    let floor: i64 = 200;
    let tolerance = std::cmp::max(floor, baseline / 4);
    if delta.abs() <= tolerance {
        DriftClass::Steady
    } else if delta < 0 {
        DriftClass::Improving
    } else {
        DriftClass::Degrading
    }
}

fn classify_success(total: usize, delta: Option<f64>) -> DriftClass {
    if total < MIN_TOTAL_RUNS_FOR_DELTA {
        return DriftClass::NotEnoughData;
    }
    let Some(delta) = delta else {
        return DriftClass::NotEnoughData;
    };
    // 10 percentage points tolerance.
    if delta.abs() <= 0.10 {
        DriftClass::Steady
    } else if delta > 0.0 {
        DriftClass::Improving
    } else {
        DriftClass::Degrading
    }
}

fn classify_refusal(total: usize, delta: Option<f64>) -> DriftClass {
    if total < MIN_TOTAL_RUNS_FOR_DELTA {
        return DriftClass::NotEnoughData;
    }
    let Some(delta) = delta else {
        return DriftClass::NotEnoughData;
    };
    // 10 percentage points tolerance, but direction is INVERTED —
    // a rising refusal rate is degrading, a falling rate is improving.
    if delta.abs() <= 0.10 {
        DriftClass::Steady
    } else if delta < 0.0 {
        DriftClass::Improving
    } else {
        DriftClass::Degrading
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use springtale_store::schema::executions::{ExecutionMode, MomentumTag};

    fn summary(id: &str, duration: Option<i64>, status: ExecutionStatus) -> ExecutionSummary {
        ExecutionSummary {
            id: id.into(),
            bot_id: Some("bot".into()),
            formation_id: None,
            rule_id: Some("rule".into()),
            recipe_id: Some("recipe".into()),
            started_at: 0,
            finished_at: duration,
            mode: ExecutionMode::Manual,
            status,
            momentum: Some(MomentumTag::Warming),
            trigger_summary: None,
            duration_ms: duration,
            error_kind: None,
        }
    }

    #[test]
    fn median_empty_is_none() {
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn median_odd_returns_middle() {
        assert_eq!(median(&[3, 1, 2]), Some(2));
    }

    #[test]
    fn median_even_returns_average() {
        assert_eq!(median(&[1, 2, 3, 4]), Some(2));
    }

    #[test]
    fn percentile_at_boundary() {
        let v = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(percentile(&v, 95), Some(100));
        assert_eq!(percentile(&v, 50), Some(60));
    }

    #[test]
    fn latency_classification_tolerates_small_swings() {
        // baseline 1000ms, recent 1100ms → 10% swing → Steady.
        assert_eq!(
            classify_latency(10, Some(100), Some(1000)),
            DriftClass::Steady
        );
    }

    #[test]
    fn latency_classification_flags_big_regression() {
        // baseline 1000ms, recent 2000ms → 100% swing → Degrading.
        assert_eq!(
            classify_latency(10, Some(1000), Some(1000)),
            DriftClass::Degrading
        );
    }

    #[test]
    fn latency_classification_flags_improvement() {
        // baseline 1000ms, recent 200ms → -80% swing → Improving.
        assert_eq!(
            classify_latency(10, Some(-800), Some(1000)),
            DriftClass::Improving
        );
    }

    #[test]
    fn latency_classification_floor_below_200ms() {
        // baseline 100ms, recent 120ms → 20ms swing → Steady
        // (below 200ms floor).
        assert_eq!(
            classify_latency(10, Some(20), Some(100)),
            DriftClass::Steady
        );
        // baseline 100ms, recent 400ms → 300ms swing → Degrading.
        assert_eq!(
            classify_latency(10, Some(300), Some(100)),
            DriftClass::Degrading
        );
    }

    #[test]
    fn success_classification_tolerates_small_drift() {
        assert_eq!(classify_success(10, Some(0.05)), DriftClass::Steady);
        assert_eq!(classify_success(10, Some(-0.05)), DriftClass::Steady);
    }

    #[test]
    fn success_classification_flags_big_drift() {
        // Recent +30 percentage points → Improving.
        assert_eq!(classify_success(10, Some(0.30)), DriftClass::Improving);
        // Recent -30 → Degrading.
        assert_eq!(classify_success(10, Some(-0.30)), DriftClass::Degrading);
    }

    #[test]
    fn refusal_classification_inverts_direction() {
        // Refusal rate UP is Degrading, DOWN is Improving.
        assert_eq!(classify_refusal(10, Some(0.25)), DriftClass::Degrading);
        assert_eq!(classify_refusal(10, Some(-0.25)), DriftClass::Improving);
    }

    #[test]
    fn worse_of_picks_degrading() {
        assert_eq!(
            DriftClass::worse_of(DriftClass::Steady, DriftClass::Degrading),
            DriftClass::Degrading
        );
        assert_eq!(
            DriftClass::worse_of(DriftClass::Improving, DriftClass::Improving),
            DriftClass::Improving
        );
        assert_eq!(
            DriftClass::worse_of(DriftClass::NotEnoughData, DriftClass::Improving),
            DriftClass::Improving
        );
    }

    #[test]
    fn build_report_aggregates_classes() {
        let recent: Vec<ExecutionSummary> = (0..5)
            .map(|i| summary(&format!("r{i}"), Some(1000), ExecutionStatus::Succeeded))
            .collect();
        let baseline: Vec<ExecutionSummary> = (0..5)
            .map(|i| summary(&format!("b{i}"), Some(1100), ExecutionStatus::Succeeded))
            .collect();
        let refusal = RateDrift {
            recent: Some(0.0),
            baseline: Some(0.0),
            delta: Some(0.0),
            class: DriftClass::Steady,
        };
        let report = build_report(&recent, &baseline, refusal);
        assert_eq!(report.latency.class, DriftClass::Steady);
        assert_eq!(report.success_rate.class, DriftClass::Steady);
        assert_eq!(report.overall, DriftClass::Steady);
        assert_eq!(report.recent_runs, 5);
        assert_eq!(report.baseline_runs, 5);
    }

    #[tokio::test]
    async fn recipe_drift_handles_empty_store() {
        use springtale_store::SqliteBackend;
        let store: Arc<dyn StorageBackend> = Arc::new(SqliteBackend::open_in_memory().unwrap());
        let report = recipe_drift(&store, "missing-recipe", DriftFilter::default())
            .await
            .unwrap();
        assert_eq!(report.recent_runs, 0);
        assert_eq!(report.baseline_runs, 0);
        assert_eq!(report.overall, DriftClass::NotEnoughData);
    }
}
