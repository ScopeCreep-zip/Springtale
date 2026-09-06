use std::time::Instant;

/// Current pacing phase for a formation — Booth's four Director phases
/// (GDC 2009, slide 81) plus the Overcooked-style interrupt.
///
/// Timestamps come from the manager's clock, which advances by the
/// elapsed time handed to [`super::PacingManager::observe`] — so phase
/// timers run on the formation's processed-tick time, not the bus rate.
pub enum PacingPhase {
    /// Booth: "Create full threat population until Survivor Intensity
    /// crosses peak threshold."
    BuildUp { started: Instant },
    /// Booth: "Continue full threat population for 3-5 seconds after
    /// Survivor Intensity has peaked."
    SustainPeak { peaked_at: Instant },
    /// Booth: "Switch to minimal threat population and monitor Survivor
    /// Intensity until it decays out of peak range."
    PeakFade { since: Instant },
    /// Booth: "Maintain minimal threat population for 30-45 seconds...
    /// then resume Build Up."
    Relax { until: Instant },
    /// Overcooked: "disruptions... force a pause-and-adapt moment"
    /// (design PDF §22.1).
    Disruption { event: String },
}

pub struct PacingTransition {
    pub from: &'static str,
    pub to: &'static str,
}
