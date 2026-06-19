// Desktop-as-controller domain: pair with remote hosts and drive their domains
// over the controller transport. Browsing/creating directories on a host backs
// the add-project remote flow; routing a hosted project's other domains builds
// on `controller_for`.

impl RuntimeService {
    /// Pair with a remote host from a pasted `codux://pair` ticket, persist it,
    /// and cache the live connection.
    pub fn pair_remote_host(
        &self,
        ticket: &str,
        device_name: &str,
    ) -> Result<crate::remote::SavedRemoteHost, String> {
        self.remote_controllers.pair(ticket, device_name)
    }

    /// Every host this desktop has paired with and can reconnect to.
    pub fn saved_remote_hosts(&self) -> Vec<crate::remote::SavedRemoteHost> {
        self.remote_controllers.saved_hosts()
    }

    /// Drop a paired host and any live connection to it.
    pub fn forget_remote_host(
        &self,
        device_id: &str,
    ) -> Result<Vec<crate::remote::SavedRemoteHost>, String> {
        self.remote_controllers.forget(device_id)
    }

    /// List a directory on a remote host (for the add-project remote browser),
    /// parsed into a typed listing so the UI never touches the wire JSON.
    pub fn remote_browse_directory(
        &self,
        device_id: &str,
        path: Option<&str>,
    ) -> Result<crate::remote::RemoteDirectoryListing, String> {
        self.remote_controllers
            .controller_for(device_id)?
            .browse_directory(path)
    }

    /// Create a directory on a remote host (for the add-project remote flow).
    pub fn remote_create_directory(
        &self,
        device_id: &str,
        path: &str,
    ) -> Result<serde_json::Value, String> {
        self.remote_controllers
            .controller_for(device_id)?
            .create_directory(path)
    }

    /// Fetch a remote host's identity/capabilities (also a reachability check).
    pub fn remote_host_info(&self, device_id: &str) -> Result<serde_json::Value, String> {
        self.remote_controllers.controller_for(device_id)?.host_info()
    }
}
