use tokio::sync::broadcast;

use crate::contract_net::types::{CallForProposals, CnError};

/// Broadcast a CFP to all subscribed participants. Takes the raw sender so
/// the initiator can share just its write half without handing out the full
/// `CfpChannels` surface.
pub fn announce(
    cfp_tx: &broadcast::Sender<CallForProposals>,
    cfp: CallForProposals,
) -> Result<(), CnError> {
    let id = cfp.id;
    cfp_tx
        .send(cfp)
        .map(|_| ())
        .map_err(|_| CnError::NotFound(id))
}
