//! apps/springtale-cli/examples/telegram-bot.rs
//!
//! Worked example §7.3 from `docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md`.
//! Pattern for a Telegram bot that spawns a 3-role formation per incoming
//! message (responder + memory_keeper + moderator) and gates the reply
//! on a consensus vote.
//!
//! Usage:
//!
//!     # Offline pattern demo — simulated messages, no real Telegram:
//!     cargo run -p springtale-cli --example telegram-bot
//!
//!     # Real bot run (production is `springtale server start` with a
//!     # `telegram-bot` template):
//!     export TELEGRAM_BOT_TOKEN=xxxxxxx
//!     cargo run -p springtale-cli --example telegram-bot -- --live
//!
//! This file is the integration *pattern* — how formation-per-message
//! looks, not how to run a real Telegram bot at scale. For a
//! production-ready bot use `springtale new telegram-bot` which ships
//! the same pattern wired into the rules engine + vault + Sentinel
//! audit trail.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use clap::Parser;
use secrecy::SecretBox;

use connector_telegram::{TelegramConfig, TelegramConnector};
use springtale_ai::{NoopAdapter};
use springtale_bot::cooperation::formation::{Formation, FormationDeps, FormationMember};
use springtale_cooperation::awareness::InMemoryGossipStore;
use springtale_cooperation::cadence::{AgentId, CadenceBus};
use springtale_cooperation::handoff::FlexibleChainPool;
use springtale_cooperation::types::FormationConstraints;
use springtale_cooperation::IntentPattern;
use springtale_store::backend::InMemoryBackend;

#[derive(Parser, Debug)]
#[command(name = "telegram-bot", about = "Telegram bot formation pattern (plan §7.3)")]
struct Args {
    /// Attempt to talk to the real Telegram API (requires
    /// `TELEGRAM_BOT_TOKEN` in the environment). Without it the demo
    /// runs a simulated message stream offline.
    #[arg(long)]
    live: bool,
}

/// Simulated inbound message — mirrors the shape of
/// `TelegramMessage` in the plan's §7.3 snippet.
#[derive(Debug, Clone)]
struct SimulatedMessage {
    id: u64,
    chat_id: i64,
    text: String,
}

/// Per-message formation. Three roles, per plan §7.3:
/// - responder (primary reply)
/// - memory_keeper (records into session memory)
/// - moderator (votes on whether to send)
async fn handle_message(msg: SimulatedMessage, cadence: Arc<CadenceBus>) {
    let deps = FormationDeps {
        cadence: cadence.clone(),
        store: Arc::new(InMemoryBackend::new()),
        gossip_store: Arc::new(InMemoryGossipStore::new()),
        flex_chain_pool: Arc::new(FlexibleChainPool::new()),
        formation_gossip: None,
    };
    let members = vec![
        FormationMember::new(AgentId::new(), vec!["telegram.responder".into()]),
        FormationMember::new(AgentId::new(), vec!["telegram.memory_keeper".into()]),
        FormationMember::new(AgentId::new(), vec!["telegram.moderator".into()]),
    ];
    let (formation, _proto, _ack) = Formation::new(
        members,
        IntentPattern::Execute { plan_id: None },
        FormationConstraints::default(),
        deps,
    );
    let formation = Arc::new(formation);

    tracing::info!(
        formation_id = %formation.id,
        msg_id = msg.id,
        chat_id = msg.chat_id,
        "spawned per-message formation"
    );

    // In a real bot each role would run on a tick loop + consensus gate.
    // This example elides that to stay focused on the *pattern*: a fresh
    // formation is built per message, the three roles cooperate, the
    // moderator gates the reply.
    let reply = generate_reply(&msg.text);
    let moderator_approves = moderator_vote(&reply);

    if moderator_approves {
        println!(
            "  [{}] reply approved → send to chat {}: {}",
            formation.id, msg.chat_id, reply
        );
    } else {
        println!(
            "  [{}] reply BLOCKED by moderator for chat {}",
            formation.id, msg.chat_id
        );
    }
}

/// Responder role — generates a reply for the incoming text.
fn generate_reply(text: &str) -> String {
    if text.starts_with("/start") {
        "Welcome to the Springtale Telegram bot demo.".to_owned()
    } else if text.starts_with("/help") {
        "Commands: /start, /help, /echo <text>".to_owned()
    } else if let Some(rest) = text.strip_prefix("/echo ") {
        format!("echo: {rest}")
    } else {
        format!("I don't know how to respond to: {text}")
    }
}

/// Moderator role — per plan §7.3, votes on whether the responder's
/// reply should actually go out. Simple heuristic for the offline demo.
fn moderator_vote(reply: &str) -> bool {
    !reply.is_empty() && reply.len() < 1024
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("springtale=info,cooperation=info")
        .init();
    let args = Args::parse();

    // 1. Build cadence bus — all per-message formations share the same
    //    drumbeat, which is how the spec avoids clock drift across
    //    concurrent conversations.
    let (bus, _reports_rx) = CadenceBus::default_30hz();
    let bus = Arc::new(bus);

    // 2. Connect to Telegram (live) or simulate offline.
    if args.live {
        let token = std::env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| "TELEGRAM_BOT_TOKEN not set — rerun without --live for offline demo")?;
        let config = TelegramConfig {
            bot_token: SecretBox::new(Box::new(token)),
            api_base: "https://api.telegram.org".to_owned(),
            update_mode: "polling".into(),
            webhook_url: None,
            webhook_secret: None,
            poll_timeout: 30,
        };
        let _connector = TelegramConnector::new(&config)?;
        println!("Telegram connector constructed (live mode).");
        println!(
            "Production wiring: use `springtale new telegram-bot`, which \
            plugs the connector into the rules engine, vault, and Sentinel. \
            This example demonstrates the per-message formation pattern \
            without running a real polling loop."
        );
        // `NoopAdapter` is named here so the example doubles as a
        // reference: when no LLM is configured the responder role
        // still runs — it just returns canned text.
        let _ = NoopAdapter;
        return Ok(());
    }

    // 3. Offline simulation: three messages through the formation pattern.
    let simulated = vec![
        SimulatedMessage {
            id: 1,
            chat_id: 42,
            text: "/start".into(),
        },
        SimulatedMessage {
            id: 2,
            chat_id: 42,
            text: "/help".into(),
        },
        SimulatedMessage {
            id: 3,
            chat_id: 99,
            text: "/echo hello from springtale".into(),
        },
    ];

    println!("\n── telegram-bot pattern demo (offline) ──");
    for msg in simulated {
        handle_message(msg, bus.clone()).await;
    }
    println!("\nDone. For a real Telegram bot, run:\n  springtale new telegram-bot");

    Ok(())
}
