use std::collections::HashSet;

use crate::{
	invalidate_query,
	node::config::{P2PDiscoveryState, Port},
};

use sd_prisma::prisma::{instance, location};

use rspc::{alpha::AlphaRouter, ErrorCode};
use serde::Deserialize;
use specta::Type;
use tracing::error;
use uuid::Uuid;

use super::{locations::ExplorerItem, utils::library, Ctx, R};

pub(crate) fn mount() -> AlphaRouter<Ctx> {
	R.router()
		.procedure("edit", {
			#[derive(Deserialize, Type)]
			pub struct ChangeNodeNameArgs {
				pub name: Option<String>,
				pub p2p_port: Option<Port>,
				pub p2p_disabled: Option<bool>,
				pub p2p_ipv6_disabled: Option<bool>,
				pub p2p_relay_disabled: Option<bool>,
				pub p2p_discovery: Option<P2PDiscoveryState>,
				pub p2p_remote_access: Option<bool>,
				pub p2p_manual_peers: Option<HashSet<String>>,
			}
			R.mutation(|node, args: ChangeNodeNameArgs| async move {
				if let Some(name) = &args.name {
					if name.is_empty() || name.len() > 250 {
						return Err(rspc::Error::new(
							ErrorCode::BadRequest,
							"invalid node name".into(),
						));
					}
				}

				node.config
					.write(|config| {
						if let Some(name) = args.name {
							config.name = name;
						}

						if let Some(port) = args.p2p_port {
							config.p2p.port = port;
						};
						if let Some(enabled) = args.p2p_disabled {
							config.p2p.disabled = enabled;
						};
						if let Some(enabled) = args.p2p_ipv6_disabled {
							config.p2p.disable_ipv6 = enabled;
						};
						if let Some(enabled) = args.p2p_relay_disabled {
							config.p2p.disable_relay = enabled;
						};
						if let Some(discovery) = args.p2p_discovery {
							config.p2p.discovery = discovery;
						};
						if let Some(remote_access) = args.p2p_remote_access {
							config.p2p.enable_remote_access = remote_access;
						};
						if let Some(manual_peers) = args.p2p_manual_peers {
							config.p2p.manual_peers = manual_peers;
						};
					})
					.await
					.map_err(|e| {
						error!(?e, "Failed to write config;");
						rspc::Error::new(
							ErrorCode::InternalServerError,
							"error updating config".into(),
						)
					})?;

				// This is a no-op if the config didn't change
				node.p2p.on_node_config_change().await;

				invalidate_query!(node; node, "nodeState");

				Ok(())
			})
		})
		// TODO: add pagination!! and maybe ordering etc
		.procedure("listLocations", {
			R.with2(library())
				// TODO: I don't like this. `node_id` should probs be a machine hash or something cause `node_id` is dynamic in the context of P2P and what does it mean for removable media to be owned by a node?
				.query(|(_, library), node_id: Option<Uuid>| async move {
					// Be aware multiple instances can exist on a single node. This is generally an edge case but it's possible.
					let instances = library
						.db
						.instance()
						.find_many(vec![node_id
							.map(|id| instance::node_id::equals(id.as_bytes().to_vec()))
							.unwrap_or(instance::id::equals(
								library.config().await.instance_id,
							))])
						.exec()
						.await?;

					Ok(library
						.db
						.location()
						.find_many(
							instances
								.into_iter()
								.map(|i| location::instance_id::equals(Some(i.id)))
								.collect(),
						)
						.exec()
						.await?
						.into_iter()
						.map(|location| ExplorerItem::Location { item: location })
						.collect::<Vec<_>>())
				})
		})
		.procedure("updateThumbnailerPreferences", {
			#[derive(Deserialize, Type)]
			pub struct UpdateThumbnailerPreferences {
				// pub background_processing_percentage: u8, // 0-100
			}
			R.mutation(
				|node, UpdateThumbnailerPreferences { .. }: UpdateThumbnailerPreferences| async move {
					node.config
						.update_preferences(|_| {
							// TODO(fogodev): introduce configurable workers count to task system
						})
						.await
						.map_err(|e| {
							error!(?e, "Failed to update thumbnailer preferences;");
							rspc::Error::with_cause(
								ErrorCode::InternalServerError,
								"Failed to update thumbnailer preferences".to_string(),
								e,
							)
						})
				},
			)
		})
}
