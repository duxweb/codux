use super::options::*;
use super::widgets::*;
use super::*;

mod overlays;
mod pane;
mod relay;

pub(in crate::app::settings) use overlays::{
    remote_connect_overlay, remote_pairing_overlay, remote_pending_pairing_overlay,
};
pub(in crate::app::settings) use pane::{SettingsRemotePaneInput, settings_remote_pane};
