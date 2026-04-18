use tokio::sync::broadcast;

use crate::contract_net::types::{Award, CnError};

/// Broadcast the winning Award to every subscriber. Rejected bidders observe
/// the `winner` field and treat non-match as rejection — no separate
/// rejection message needed.
pub fn notify(award_tx: &broadcast::Sender<Award>, award: Award) -> Result<(), CnError> {
    let id = award.cfp_id;
    award_tx
        .send(award)
        .map(|_| ())
        .map_err(|_| CnError::NotFound(id))
}
