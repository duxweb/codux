/// The active pane in the remote workspace, shared by the phone and pad
/// layouts. Replaces the stringly-typed mode that used to flow through the
/// workspace controller and every tab/header widget.
enum WorkspaceMode {
  terminal,
  stats,
  files,
  git,
  review,
  ssh,
}
