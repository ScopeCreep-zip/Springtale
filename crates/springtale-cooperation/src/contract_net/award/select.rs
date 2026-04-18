use crate::contract_net::types::Bid;

/// Pick the bid with the highest utility score. Ties broken by bidder uuid
/// (stable ordering so results are deterministic across runs). Returns
/// `None` on empty input.
pub fn highest_utility(bids: &[Bid]) -> Option<&Bid> {
    bids.iter().max_by(|a, b| {
        a.utility
            .total_cmp(&b.utility)
            .then_with(|| a.bidder.0.cmp(&b.bidder.0))
    })
}
