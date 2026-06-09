use super::crypto::remote_pairing_match_code;
use super::summary::remote_summary_from_settings;
use super::types::{RemotePairingInfo, RemotePendingPairing, RemoteSettings, RemoteSummary};

pub(crate) fn remote_summary_show_pending_pairing(
    settings: RemoteSettings,
    active_pairing: &RemotePairingInfo,
    pairing_id: String,
    device_name: String,
    device_public_key: String,
    pairing_code: String,
    pairing_secret: String,
) -> RemoteSummary {
    let mut summary = remote_summary_from_settings(settings.clone());
    if pairing_id.trim().is_empty() {
        summary.pairing = Some(active_pairing.clone());
        return summary;
    }

    let match_code = remote_pairing_match_code(
        &settings,
        &pairing_code,
        &pairing_secret,
        &device_public_key,
    )
    .unwrap_or(pairing_code);

    summary.status = "connected".to_string();
    summary.message = "Confirm device pairing.".to_string();
    summary.pending_pairing_list.push(RemotePendingPairing {
        id: pairing_id,
        device_name,
        device_public_key,
        code: match_code,
    });
    summary.pending_pairings = summary.pending_pairing_list.len();
    summary
}
