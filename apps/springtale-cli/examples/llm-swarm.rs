//! apps/springtale-cli/examples/llm-swarm.rs
//!
//! Worked example §7.2 from `docs/intended-arch/COOPERATION_IMPLEMENTATION_PLAN.md`.
//! Three specialized agents (researcher / writer / critic) cooperate on
//! a single prompt. The cooperation module drives the cadence; the AI
//! adapter drives the content.
//!
//! Usage:
//!
//!     cargo run -p springtale-cli --example llm-swarm -- \
//!         "explain the Necrodancer rollback model"
//!
//!     # Force NoopAdapter (offline smoke test):
//!     cargo run -p springtale-cli --example llm-swarm -- --no-ai "..."
//!
//!     # Specify an Ollama base URL / model:
//!     cargo run -p springtale-cli --example llm-swarm -- \
//!         --ollama-url http://localhost:11434 --model llama3.2 "..."
//!
//! Pipeline: researcher gathers raw notes → writer synthesizes a draft
//! → critic reviews. When the AI adapter is `NoopAdapter` (no AI
//! configured), each step returns a canned response so the full
//! pipeline still exercises formation lifecycle + handoff plumbing.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use clap::Parser;

use springtale_ai::{
    AiAdapter, AiOptions, AiRequest, ChatMessage, NoopAdapter, OllamaConfig,
};
use springtale_ai::ollama::OllamaAdapter;
use springtale_bot::cooperation::formation::{Formation, FormationDeps, FormationMember};
use springtale_cooperation::awareness::InMemoryGossipStore;
use springtale_cooperation::cadence::{AgentId, CadenceBus};
use springtale_cooperation::handoff::FlexibleChainPool;
use springtale_cooperation::types::FormationConstraints;
use springtale_cooperation::{IntentPattern, PlanId};
use springtale_store::backend::InMemoryBackend;

#[derive(Parser, Debug)]
#[command(name = "llm-swarm", about = "3-agent LLM orchestration swarm (plan §7.2)")]
struct Args {
    /// User prompt the swarm will research, write about, and critique.
    prompt: String,

    /// Force the `NoopAdapter` even if Ollama is reachable.
    #[arg(long)]
    no_ai: bool,

    /// Ollama base URL (only used when `--no-ai` is not set).
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,

    /// Model identifier passed to the provider.
    #[arg(long, default_value = "llama3.2")]
    model: String,
}

/// Pick the AI adapter. Tries Ollama if requested; falls back to
/// `NoopAdapter` on any error so the example always runs.
fn pick_adapter(args: &Args) -> Arc<dyn AiAdapter> {
    if args.no_ai {
        tracing::info!("using NoopAdapter (--no-ai)");
        return Arc::new(NoopAdapter);
    }
    match OllamaAdapter::new(OllamaConfig {
        base_url: args.ollama_url.clone(),
        model: args.model.clone(),
    }) {
        Ok(adapter) => {
            tracing::info!(url = %args.ollama_url, model = %args.model, "using Ollama");
            Arc::new(adapter)
        }
        Err(e) => {
            tracing::warn!(error = %e, "ollama unavailable — falling back to NoopAdapter");
            Arc::new(NoopAdapter)
        }
    }
}

/// Drive one role in the pipeline. Returns the agent's text output.
async fn run_role(
    role: &str,
    system_prompt: &str,
    user_prompt: &str,
    adapter: &dyn AiAdapter,
) -> String {
    let request = AiRequest::Chat {
        messages: vec![
            ChatMessage::text("system", system_prompt.to_owned()),
            ChatMessage::text("user", user_prompt.to_owned()),
        ],
    };
    match adapter.complete(request, AiOptions::default()).await {
        Ok(response) => response.content,
        Err(e) => {
            // NoopAdapter returns `AiError::Disabled` — that's the
            // expected no-AI path. Stamp a canned line for the role so
            // downstream stages still have something to operate on.
            tracing::debug!(role, error = %e, "adapter returned error; using canned text");
            match role {
                "researcher" => format!("[noop researcher] gathered notes on: {user_prompt}"),
                "writer" => format!("[noop writer] draft based on notes: {user_prompt}"),
                "critic" => format!("[noop critic] looks fine to me: {user_prompt}"),
                _ => format!("[noop {role}] {user_prompt}"),
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("springtale=info,cooperation=info")
        .init();
    let args = Args::parse();
    let adapter = pick_adapter(&args);

    // 1. Cadence + formation wiring. Same shape as task-runner.rs —
    //    formation supplies the shared clock + member bus + rally.
    let (bus, _reports_rx) = CadenceBus::default_30hz();
    let bus = Arc::new(bus);
    let deps = FormationDeps {
        cadence: bus.clone(),
        store: Arc::new(InMemoryBackend::new()),
        gossip_store: Arc::new(InMemoryGossipStore::new()),
        flex_chain_pool: Arc::new(FlexibleChainPool::new()),
        formation_gossip: None,
    };
    let members = vec![
        FormationMember::new(AgentId::new(), vec!["research".into()]),
        FormationMember::new(AgentId::new(), vec!["writing".into()]),
        FormationMember::new(AgentId::new(), vec!["review".into()]),
    ];
    let (formation, _proto, _ack) = Formation::new(
        members,
        IntentPattern::Execute {
            plan_id: Some(PlanId::from(format!("llm-swarm-{}", &args.prompt))),
        },
        FormationConstraints::default(),
        deps,
    );
    let formation = Arc::new(formation);
    tracing::info!(
        formation_id = %formation.id,
        roles = formation.members.len(),
        "formation assembled — Cold tier, Execute intent"
    );

    // 2. Role system prompts. Kept short for readability; real-world
    //    agents would load these from the bot config + per-role
    //    capability declarations.
    let researcher_sys = "You are a researcher. List bullet points of what the user asks about. Be concise.";
    let writer_sys = "You are a writer. Given research notes, draft a clear 2-paragraph explanation.";
    let critic_sys = "You are a critic. Given a draft, flag inaccuracies and suggest one improvement.";

    // 3. Pipeline: researcher → writer → critic. Handoff is a plain
    //    `String` between calls here; a production bot routes this
    //    through `HandoffType::Sequential` on the formation bus (§20).
    println!("\n── research phase ──");
    let notes = run_role("researcher", researcher_sys, &args.prompt, adapter.as_ref()).await;
    println!("{notes}");

    println!("\n── write phase ──");
    let draft = run_role("writer", writer_sys, &notes, adapter.as_ref()).await;
    println!("{draft}");

    println!("\n── critique phase ──");
    let critique = run_role("critic", critic_sys, &draft, adapter.as_ref()).await;
    println!("{critique}");

    // 4. Summary.
    println!("\n── llm-swarm summary ──");
    println!("  prompt       : {}", args.prompt);
    println!("  formation id : {}", formation.id);
    println!("  adapter      : {}", std::any::type_name_of_val(adapter.as_ref()));
    println!("  stages       : research → write → critique");

    Ok(())
}
