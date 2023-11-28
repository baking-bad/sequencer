// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::NetworkConnectionMetrics;
use anemo::types::PeerEvent;
use anemo::PeerId;
use dashmap::DashMap;
use futures::future;
use mysten_metrics::spawn_logged_monitored_task;
use quinn_proto::ConnectionStats;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time;
use types::ConditionalBroadcastReceiver;

const CONNECTION_STAT_COLLECTION_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
}

pub struct ConnectionMonitor {
    network: anemo::NetworkRef,
    connection_metrics: NetworkConnectionMetrics,
    peer_id_types: HashMap<PeerId, String>,
    connection_statuses: Arc<DashMap<PeerId, ConnectionStatus>>,
    rx_shutdown: Option<ConditionalBroadcastReceiver>,
}

impl ConnectionMonitor {
    #[must_use]
    pub fn spawn(
        network: anemo::NetworkRef,
        connection_metrics: NetworkConnectionMetrics,
        peer_id_types: HashMap<PeerId, String>,
        rx_shutdown: Option<ConditionalBroadcastReceiver>,
    ) -> (JoinHandle<()>, Arc<DashMap<PeerId, ConnectionStatus>>) {
        let connection_statuses_outer = Arc::new(DashMap::new());
        let connection_statuses = connection_statuses_outer.clone();
        (
            spawn_logged_monitored_task!(
                Self {
                    network,
                    connection_metrics,
                    peer_id_types,
                    connection_statuses,
                    rx_shutdown
                }
                .run(),
                "ConnectionMonitor"
            ),
            connection_statuses_outer,
        )
    }

    async fn run(mut self) {
        let (mut subscriber, connected_peers) = {
            if let Some(network) = self.network.upgrade() {
                let Ok((subscriber, active_peers)) = network.subscribe() else {
                    return;
                };
                (subscriber, active_peers)
            } else {
                return;
            }
        };

        // we report first all the known peers as disconnected - so we can see
        // their labels in the metrics reporting tool
        let mut known_peers = Vec::new();
        for (peer_id, ty) in &self.peer_id_types {
            known_peers.push(*peer_id);
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&format!("{peer_id}"), ty])
                .set(0)
        }

        // now report the connected peers
        for peer_id in connected_peers.iter() {
            self.handle_peer_event(PeerEvent::NewPeer(*peer_id)).await;
        }

        let mut connection_stat_collection_interval =
            time::interval(CONNECTION_STAT_COLLECTION_INTERVAL);

        async fn wait_for_shutdown(
            rx_shutdown: &mut Option<ConditionalBroadcastReceiver>,
        ) -> Result<(), tokio::sync::broadcast::error::RecvError> {
            if let Some(rx) = rx_shutdown.as_mut() {
                rx.receiver.recv().await
            } else {
                // If no shutdown receiver is provided, wait forever.
                let future = future::pending();
                #[allow(clippy::let_unit_value)]
                let () = future.await;
                Ok(())
            }
        }

        loop {
            tokio::select! {
                _ = connection_stat_collection_interval.tick() => {
                    if let Some(network) = self.network.upgrade() {
                        self.connection_metrics.socket_receive_buffer_size.set(
                            network.socket_receive_buf_size() as i64
                        );
                        self.connection_metrics.socket_send_buffer_size.set(
                            network.socket_send_buf_size() as i64
                        );
                        for peer_id in known_peers.iter() {
                            if let Some(connection) = network.peer(*peer_id) {
                                let stats = connection.connection_stats();
                                self.update_quinn_metrics_for_peer(&format!("{peer_id}"), &stats);
                            }
                        }
                    } else {
                        continue;
                    }
                }
                Ok(event) = subscriber.recv() => {
                    self.handle_peer_event(event).await;
                }
                _ = wait_for_shutdown(&mut self.rx_shutdown) => {
                    return;
                }
            }
        }
    }

    async fn handle_peer_event(&self, peer_event: PeerEvent) {
        if let Some(network) = self.network.upgrade() {
            self.connection_metrics
                .network_peers
                .set(network.peers().len() as i64);
        } else {
            return;
        }

        let (peer_id, status, int_status) = match peer_event {
            PeerEvent::NewPeer(peer_id) => (peer_id, ConnectionStatus::Connected, 1),
            PeerEvent::LostPeer(peer_id, _) => (peer_id, ConnectionStatus::Disconnected, 0),
        };
        self.connection_statuses.insert(peer_id, status);

        // Only report peer IDs for known peers to prevent unlimited cardinality.
        let peer_id_str = if self.peer_id_types.contains_key(&peer_id) {
            format!("{peer_id}")
        } else {
            "other_peer".to_string()
        };

        if let Some(ty) = self.peer_id_types.get(&peer_id) {
            self.connection_metrics
                .network_peer_connected
                .with_label_values(&[&peer_id_str, ty])
                .set(int_status);
        }

        if let PeerEvent::LostPeer(_, reason) = peer_event {
            self.connection_metrics
                .network_peer_disconnects
                .with_label_values(&[&peer_id_str, &format!("{reason:?}")])
                .inc();
        }
    }

    // TODO: Replace this with ClosureMetric
    fn update_quinn_metrics_for_peer(&self, peer_id: &str, stats: &ConnectionStats) {
        // Update PathStats
        self.connection_metrics
            .network_peer_rtt
            .with_label_values(&[peer_id])
            .set(stats.path.rtt.as_millis() as i64);
        self.connection_metrics
            .network_peer_lost_packets
            .with_label_values(&[peer_id])
            .set(stats.path.lost_packets as i64);
        self.connection_metrics
            .network_peer_lost_bytes
            .with_label_values(&[peer_id])
            .set(stats.path.lost_bytes as i64);
        self.connection_metrics
            .network_peer_sent_packets
            .with_label_values(&[peer_id])
            .set(stats.path.sent_packets as i64);
        self.connection_metrics
            .network_peer_congestion_events
            .with_label_values(&[peer_id])
            .set(stats.path.congestion_events as i64);
        self.connection_metrics
            .network_peer_congestion_window
            .with_label_values(&[peer_id])
            .set(stats.path.cwnd as i64);

        // Update FrameStats
        self.connection_metrics
            .network_peer_max_data
            .with_label_values(&[peer_id, "transmitted"])
            .set(stats.frame_tx.max_data as i64);
        self.connection_metrics
            .network_peer_max_data
            .with_label_values(&[peer_id, "received"])
            .set(stats.frame_rx.max_data as i64);
        self.connection_metrics
            .network_peer_closed_connections
            .with_label_values(&[peer_id, "transmitted"])
            .set(stats.frame_tx.connection_close as i64);
        self.connection_metrics
            .network_peer_closed_connections
            .with_label_values(&[peer_id, "received"])
            .set(stats.frame_rx.connection_close as i64);
        self.connection_metrics
            .network_peer_data_blocked
            .with_label_values(&[peer_id, "transmitted"])
            .set(stats.frame_tx.data_blocked as i64);
        self.connection_metrics
            .network_peer_data_blocked
            .with_label_values(&[peer_id, "received"])
            .set(stats.frame_rx.data_blocked as i64);

        // Update UDPStats
        self.connection_metrics
            .network_peer_udp_datagrams
            .with_label_values(&[peer_id, "transmitted"])
            .set(stats.udp_tx.datagrams as i64);
        self.connection_metrics
            .network_peer_udp_datagrams
            .with_label_values(&[peer_id, "received"])
            .set(stats.udp_rx.datagrams as i64);
        self.connection_metrics
            .network_peer_udp_bytes
            .with_label_values(&[peer_id, "transmitted"])
            .set(stats.udp_tx.bytes as i64);
        self.connection_metrics
            .network_peer_udp_bytes
            .with_label_values(&[peer_id, "received"])
            .set(stats.udp_rx.bytes as i64);
        self.connection_metrics
            .network_peer_udp_transmits
            .with_label_values(&[peer_id, "transmitted"])
            .set(stats.udp_tx.transmits as i64);
        self.connection_metrics
            .network_peer_udp_transmits
            .with_label_values(&[peer_id, "received"])
            .set(stats.udp_rx.transmits as i64);
    }
}
