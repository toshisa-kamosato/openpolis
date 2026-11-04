// SPDX-License-Identifier: GPL-2.0-or-later
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use libp2p::{
    autonat, dcutr, gossipsub, identify, identity::Keypair, kad, mdns, multiaddr::Protocol, noise, ping, relay,
    swarm::{behaviour::toggle::Toggle, NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, warn};

const PROTOCOL: &str = "/lux/1.0.0";
const HOP_PROTOCOL: &str = "/libp2p/circuit/relay/0.2.0/hop";
const INBOX_MAX: usize = 200;

#[derive(Debug, Clone)]
pub struct NodeConfig {
    pub listen: Vec<Multiaddr>,
    pub bootstrap: Vec<Multiaddr>,
    pub relay_server: bool,
    pub mdns: bool,
    pub topic: String,
    pub agent: String,
}

impl NodeConfig {
    pub fn from_core(c: &lux_core::config::Config) -> Result<NodeConfig> {
        let parse = |v: &[String]| -> Result<Vec<Multiaddr>> {
            v.iter().map(|s| s.parse::<Multiaddr>().with_context(|| format!("invalid multiaddr: {s}"))).collect()
        };
        Ok(NodeConfig {
            listen: parse(&c.listen)?,
            bootstrap: parse(&c.bootstrap)?,
            relay_server: c.relay_server,
            mdns: c.mdns,
            topic: c.topic.clone(),
            agent: format!("luxd/{}", env!("CARGO_PKG_VERSION")),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub addrs: Vec<String>,
    pub agent: String,
    /// True when every known connection to this peer goes through a relay.
    pub relayed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub peer_id: String,
    pub agent: String,
    pub listen: Vec<String>,
    pub external: Vec<String>,
    pub connected_peers: usize,
    pub relay_reservations: Vec<String>,
    pub nat: String,
    pub relay_server: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMsg {
    pub from: String,
    pub text: String,
    pub at: u64,
}

#[derive(NetworkBehaviour)]
struct Behaviour {
    identify: identify::Behaviour,
    ping: ping::Behaviour,
    kad: kad::Behaviour<kad::store::MemoryStore>,
    gossipsub: gossipsub::Behaviour,
    mdns: Toggle<mdns::tokio::Behaviour>,
    autonat: autonat::Behaviour,
    relay_server: Toggle<relay::Behaviour>,
    relay_client: relay::client::Behaviour,
    dcutr: dcutr::Behaviour,
}

enum Command {
    Status(oneshot::Sender<StatusInfo>),
    Peers(oneshot::Sender<Vec<PeerInfo>>),
    Dial(Multiaddr, oneshot::Sender<Result<(), String>>),
    Say(String, oneshot::Sender<Result<(), String>>),
    Inbox(oneshot::Sender<Vec<InboxMsg>>),
    Shutdown,
}

/// Cheap, cloneable handle to a running node.
#[derive(Clone)]
pub struct NodeHandle {
    tx: mpsc::Sender<Command>,
    peer_id: PeerId,
}

impl NodeHandle {
    pub fn peer_id(&self) -> PeerId {
        self.peer_id
    }

    async fn ask<T>(&self, make: impl FnOnce(oneshot::Sender<T>) -> Command) -> Result<T> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(make(tx)).await.map_err(|_| anyhow!("node stopped"))?;
        rx.await.map_err(|_| anyhow!("node stopped"))
    }

    pub async fn status(&self) -> Result<StatusInfo> {
        self.ask(Command::Status).await
    }
    pub async fn peers(&self) -> Result<Vec<PeerInfo>> {
        self.ask(Command::Peers).await
    }
    pub async fn dial(&self, addr: Multiaddr) -> Result<()> {
        self.ask(|t| Command::Dial(addr, t)).await?.map_err(|e| anyhow!(e))
    }
    pub async fn say(&self, text: String) -> Result<()> {
        self.ask(|t| Command::Say(text, t)).await?.map_err(|e| anyhow!(e))
    }
    pub async fn inbox(&self) -> Result<Vec<InboxMsg>> {
        self.ask(Command::Inbox).await
    }
    pub async fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown).await;
    }
}

#[derive(Default)]
struct PeerState {
    addrs: HashSet<String>,
    agent: String,
}

#[derive(Default)]
struct State {
    listen: Vec<Multiaddr>,
    external: Vec<Multiaddr>,
    peers: HashMap<PeerId, PeerState>,
    reservations: Vec<PeerId>,
    inbox: VecDeque<InboxMsg>,
    nat: String,
    bootstrap: HashMap<PeerId, Multiaddr>,
    reserving: HashSet<PeerId>,
}

fn peer_of(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|p| if let Protocol::P2p(id) = p { Some(id) } else { None })
}

fn build_swarm(keypair: Keypair, cfg: &NodeConfig) -> Result<Swarm<Behaviour>> {
    let agent = cfg.agent.clone();
    let (use_mdns, relay_server) = (cfg.mdns, cfg.relay_server);
    let swarm = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_tcp(tcp::Config::default(), noise::Config::new, yamux::Config::default)?
        .with_quic()
        .with_dns()?
        .with_relay_client(noise::Config::new, yamux::Config::default)?
        .with_behaviour(move |key, relay_client| {
            let peer_id = key.public().to_peer_id();
            let identify = identify::Behaviour::new(
                identify::Config::new(PROTOCOL.into(), key.public()).with_agent_version(agent.clone()),
            );
            let mut kad = kad::Behaviour::new(peer_id, kad::store::MemoryStore::new(peer_id));
            kad.set_mode(Some(kad::Mode::Server));
            let gs_cfg = gossipsub::ConfigBuilder::default()
                .heartbeat_interval(Duration::from_secs(1))
                .validation_mode(gossipsub::ValidationMode::Strict)
                .build()
                .map_err(|e| format!("gossipsub config: {e}"))?;
            let gossipsub = gossipsub::Behaviour::new(gossipsub::MessageAuthenticity::Signed(key.clone()), gs_cfg)
                .map_err(|e| format!("gossipsub: {e}"))?;
            let mdns = if use_mdns {
                Toggle::from(Some(mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)?))
            } else {
                Toggle::from(None)
            };
            let relay_srv = if relay_server {
                Toggle::from(Some(relay::Behaviour::new(peer_id, relay::Config::default())))
            } else {
                Toggle::from(None)
            };
            Ok(Behaviour {
                identify,
                ping: ping::Behaviour::default(),
                kad,
                gossipsub,
                mdns,
                autonat: autonat::Behaviour::new(peer_id, autonat::Config::default()),
                relay_server: relay_srv,
                relay_client,
                dcutr: dcutr::Behaviour::new(peer_id),
            })
        })?
        .with_swarm_config(|c| c.with_idle_connection_timeout(Duration::from_secs(120)))
        .build();
    Ok(swarm)
}

/// Starts a node in a background task and returns a handle to it.
pub async fn spawn(keypair: Keypair, cfg: NodeConfig) -> Result<NodeHandle> {
    let peer_id = keypair.public().to_peer_id();
    let mut swarm = build_swarm(keypair, &cfg)?;
    let topic = gossipsub::IdentTopic::new(cfg.topic.clone());
    swarm.behaviour_mut().gossipsub.subscribe(&topic).map_err(|e| anyhow!("subscribe: {e}"))?;
    for addr in &cfg.listen {
        swarm.listen_on(addr.clone()).with_context(|| format!("listen on {addr}"))?;
    }
    let mut state = State { nat: "unknown".into(), ..Default::default() };
    for addr in &cfg.bootstrap {
        if let Some(pid) = peer_of(addr) {
            swarm.behaviour_mut().kad.add_address(&pid, addr.clone());
            state.bootstrap.insert(pid, addr.clone());
        } else {
            warn!(%addr, "bootstrap address without /p2p/<peer id>: ignored");
            continue;
        }
        if let Err(e) = swarm.dial(addr.clone()) {
            warn!(%addr, error = %e, "cannot dial bootstrap node");
        }
    }
    let (tx, rx) = mpsc::channel(64);
    let relay_server = cfg.relay_server;
    let agent = cfg.agent.clone();
    tokio::spawn(run(swarm, rx, topic, state, peer_id, agent, relay_server));
    Ok(NodeHandle { tx, peer_id })
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

async fn run(
    mut swarm: Swarm<Behaviour>,
    mut rx: mpsc::Receiver<Command>,
    topic: gossipsub::IdentTopic,
    mut st: State,
    local: PeerId,
    agent: String,
    relay_server: bool,
) {
    let mut kad_tick = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            cmd = rx.recv() => {
                let Some(cmd) = cmd else { break };
                match cmd {
                    Command::Shutdown => break,
                    Command::Status(r) => {
                        let _ = r.send(StatusInfo {
                            peer_id: local.to_string(),
                            agent: agent.clone(),
                            listen: st.listen.iter().map(|a| a.to_string()).collect(),
                            external: st.external.iter().map(|a| a.to_string()).collect(),
                            connected_peers: st.peers.len(),
                            relay_reservations: st.reservations.iter().map(|p| p.to_string()).collect(),
                            nat: st.nat.clone(),
                            relay_server,
                        });
                    }
                    Command::Peers(r) => {
                        let mut v: Vec<PeerInfo> = st.peers.iter().map(|(p, s)| {
                            let mut addrs: Vec<String> = s.addrs.iter().cloned().collect();
                            addrs.sort();
                            let relayed = !addrs.is_empty() && addrs.iter().all(|a| a.contains("p2p-circuit"));
                            PeerInfo { peer_id: p.to_string(), addrs, agent: s.agent.clone(), relayed }
                        }).collect();
                        v.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
                        let _ = r.send(v);
                    }
                    Command::Dial(addr, r) => {
                        let _ = r.send(swarm.dial(addr).map_err(|e| e.to_string()));
                    }
                    Command::Say(text, r) => {
                        let res = swarm.behaviour_mut().gossipsub.publish(topic.clone(), text.into_bytes())
                            .map(|_| ()).map_err(|e| e.to_string());
                        let _ = r.send(res);
                    }
                    Command::Inbox(r) => {
                        let _ = r.send(st.inbox.iter().cloned().collect());
                    }
                }
            }
            ev = swarm.select_next_some() => on_event(&mut swarm, &mut st, ev),
            _ = kad_tick.tick() => {
                let _ = swarm.behaviour_mut().kad.bootstrap();
            }
        }
    }
    debug!("node loop ended");
}

fn on_event(swarm: &mut Swarm<Behaviour>, st: &mut State, ev: SwarmEvent<BehaviourEvent>) {
    match ev {
        SwarmEvent::NewListenAddr { address, .. } => {
            info!(%address, "listening");
            st.listen.push(address);
        }
        SwarmEvent::ExpiredListenAddr { address, .. } => st.listen.retain(|a| a != &address),
        SwarmEvent::ExternalAddrConfirmed { address } => {
            info!(%address, "external address confirmed");
            st.external.push(address);
        }
        SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
            let p = st.peers.entry(peer_id).or_default();
            p.addrs.insert(endpoint.get_remote_address().to_string());
            debug!(%peer_id, "connected");
        }
        SwarmEvent::ConnectionClosed { peer_id, endpoint, num_established, .. } => {
            if num_established == 0 {
                st.peers.remove(&peer_id);
            } else if let Some(p) = st.peers.get_mut(&peer_id) {
                p.addrs.remove(&endpoint.get_remote_address().to_string());
            }
        }
        SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
            for a in &info.listen_addrs {
                swarm.behaviour_mut().kad.add_address(&peer_id, a.clone());
            }
            st.peers.entry(peer_id).or_default().agent = info.agent_version.clone();
            let hop = info.protocols.iter().any(|p| p.as_ref() == HOP_PROTOCOL);
            // A bootstrap node that offers relaying becomes our door to the world when we are behind a NAT.
            if hop && st.bootstrap.contains_key(&peer_id) && st.reserving.insert(peer_id) {
                let circuit = st.bootstrap[&peer_id].clone().with(Protocol::P2pCircuit);
                match swarm.listen_on(circuit) {
                    Ok(_) => info!(relay = %peer_id, "requesting relay reservation"),
                    Err(e) => warn!(relay = %peer_id, error = %e, "relay reservation failed"),
                }
            }
            let _ = swarm.behaviour_mut().kad.bootstrap();
        }
        SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message { propagation_source, message, .. })) => {
            let from = message.source.unwrap_or(propagation_source).to_string();
            let text = String::from_utf8_lossy(&message.data).chars().take(2000).collect::<String>();
            info!(%from, "gossip message");
            if st.inbox.len() >= INBOX_MAX {
                st.inbox.pop_front();
            }
            st.inbox.push_back(InboxMsg { from, text, at: now() });
        }
        SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
            for (peer, addr) in list {
                swarm.behaviour_mut().kad.add_address(&peer, addr.clone());
                if !st.peers.contains_key(&peer) {
                    let _ = swarm.dial(addr);
                }
            }
        }
        SwarmEvent::Behaviour(BehaviourEvent::RelayClient(relay::client::Event::ReservationReqAccepted {
            relay_peer_id, ..
        })) => {
            info!(relay = %relay_peer_id, "relay reservation accepted");
            if !st.reservations.contains(&relay_peer_id) {
                st.reservations.push(relay_peer_id);
            }
        }
        SwarmEvent::Behaviour(BehaviourEvent::Dcutr(e)) => info!(event = ?e, "hole punching"),
        SwarmEvent::Behaviour(BehaviourEvent::Autonat(autonat::Event::StatusChanged { new, .. })) => {
            st.nat = format!("{new:?}");
            info!(nat = %st.nat, "NAT status changed");
        }
        SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
            debug!(?peer_id, %error, "outgoing connection failed");
        }
        _ => {}
    }
}
