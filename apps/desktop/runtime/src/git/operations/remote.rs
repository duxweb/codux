use std::process::{Command, Stdio};

fn clone_repository_system_git(remote_url: &str, project_path: &Path) -> Result<(), String> {
    let Some(parent) = project_path.parent().filter(|path| !path.as_os_str().is_empty()) else {
        return Err("Project path must include a parent directory.".to_string());
    };
    let Some(name) = project_path.file_name().and_then(|value| value.to_str()) else {
        return Err("Project path must include a directory name.".to_string());
    };
    run_system_git(parent, &["clone", remote_url, name], None)
}

fn clone_repository_git2_with_credentials(
    remote_url: &str,
    project_path: &Path,
    credentials: GitCredentials,
) -> Result<(), String> {
    let mut fetch_options = git2::FetchOptions::new();
    fetch_options.remote_callbacks(git_remote_callbacks_with_credentials(None, Some(credentials)));
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);
    builder
        .clone(remote_url, project_path)
        .map(|_| ())
        .map_err(|error| error.message().to_string())
}

fn fetch_all_remotes_system_git(
    repo: &GitRepository,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    run_system_git(repo_root(repo), &["fetch", "--all"], cancel)
}

fn pull_current_branch_system_git(
    repo: &GitRepository,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    check_git_cancelled(cancel)?;
    let branch_name = current_branch_name(repo);
    if branch_name == "HEAD" || branch_name == "uninitialized" {
        return Err("Cannot pull detached HEAD.".to_string());
    }
    let branch = repo
        .find_branch(&branch_name, git2::BranchType::Local)
        .map_err(|error| error.message().to_string())?;
    let upstream = branch
        .upstream()
        .map_err(|_| "The current branch does not have an upstream.".to_string())?;
    let upstream_name = upstream
        .name()
        .ok()
        .flatten()
        .ok_or_else(|| "The upstream branch name is invalid.".to_string())?
        .to_string();
    upstream_name
        .split_once('/')
        .map(|(remote, _)| remote)
        .ok_or_else(|| "The upstream branch is missing a remote name.".to_string())?;
    run_system_git(repo_root(repo), &["pull", "--rebase"], cancel)?;
    check_git_cancelled(cancel)?;
    let _ = branch;
    Ok(())
}

fn push_current_branch_system_git(
    repo: &GitRepository,
    remote_override: Option<&str>,
    force: bool,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    check_git_cancelled(cancel)?;
    let branch = current_branch_name(repo);
    if branch == "HEAD" || branch == "uninitialized" {
        return Err("Cannot push detached HEAD.".to_string());
    }
    let remote = remote_override
        .map(str::to_string)
        .or_else(|| upstream_remote_for_branch(repo, &branch))
        .or_else(|| first_remote_name(repo))
        .ok_or_else(|| "No Git remote is configured.".to_string())?;
    let refspec = if force {
        format!("+refs/heads/{branch}:refs/heads/{branch}")
    } else {
        format!("refs/heads/{branch}:refs/heads/{branch}")
    };
    push_refspec_system_git(repo, &remote, &refspec, cancel)?;
    if let Ok(mut branch_ref) = repo.find_branch(&branch, git2::BranchType::Local) {
        let _ = branch_ref.set_upstream(Some(&format!("{remote}/{branch}")));
    }
    Ok(())
}

fn push_refspec_system_git(
    repo: &GitRepository,
    remote_name: &str,
    refspec: &str,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    check_git_cancelled(cancel)?;
    run_system_git(repo_root(repo), &["push", remote_name, refspec], cancel)
}

fn run_system_git(
    working_dir: &Path,
    args: &[&str],
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    check_git_cancelled(cancel)?;
    let mut command = system_git_command();
    let mut child = command
        .args(args)
        .current_dir(&working_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Failed to start system git: {error}"))?;

    loop {
        check_git_cancelled_with_child(cancel, &mut child)?;
        match child
            .try_wait()
            .map_err(|error| format!("Failed to wait for system git: {error}"))?
        {
            Some(_) => break,
            None => thread::sleep(Duration::from_millis(60)),
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("Failed to read system git output: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(normalize_git_error_message(&system_git_output_message(&output)))
}

fn system_git_command() -> Command {
    let mut command = Command::new("git");
    configure_background_command(&mut command);
    command
}

#[cfg(windows)]
fn configure_background_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn configure_background_command(_command: &mut Command) {}

fn check_git_cancelled_with_child(
    cancel: Option<&GitCancelToken>,
    child: &mut std::process::Child,
) -> Result<(), String> {
    if is_git_cancelled(cancel) {
        let _ = child.kill();
        let _ = child.wait();
        return Err("Git operation cancelled.".to_string());
    }
    Ok(())
}

fn system_git_output_message(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }
    format!("System git failed with status {}.", output.status)
}

fn fast_forward_head(repo: &GitRepository, target: git2::Oid) -> Result<(), String> {
    let head_name = repo
        .head()
        .ok()
        .and_then(|head| head.name().ok().map(str::to_string))
        .ok_or_else(|| "Cannot fast-forward detached HEAD.".to_string())?;
    let target_object = repo
        .find_object(target, None)
        .map_err(git_error_message)?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&target_object, Some(&mut checkout))
        .map_err(git_error_message)?;
    let mut reference = repo
        .find_reference(&head_name)
        .map_err(git_error_message)?;
    reference
        .set_target(target, "Fast-forward")
        .map_err(git_error_message)?;
    repo.set_head(&head_name)
        .map_err(git_error_message)?;
    Ok(())
}

fn upstream_remote_for_branch(repo: &GitRepository, branch: &str) -> Option<String> {
    let local = repo.find_branch(branch, git2::BranchType::Local).ok()?;
    let upstream = local.upstream().ok()?;
    let name = upstream.name().ok().flatten()?;
    name.split_once('/').map(|(remote, _)| remote.to_string())
}

fn first_remote_name(repo: &GitRepository) -> Option<String> {
    repo.remotes()
        .ok()?
        .iter()
        .flatten()
        .flatten()
        .next()
        .map(str::to_string)
}

fn git_remote_callbacks_with_credentials<'a>(
    cancel: Option<GitCancelToken>,
    credentials: Option<GitCredentials>,
) -> git2::RemoteCallbacks<'a> {
    let mut callbacks = git2::RemoteCallbacks::new();
    let transfer_cancel = cancel.clone();
    callbacks.transfer_progress(move |_| !is_git_cancelled(transfer_cancel.as_ref()));
    let sideband_cancel = cancel.clone();
    callbacks.sideband_progress(move |_| !is_git_cancelled(sideband_cancel.as_ref()));
    let push_negotiation_cancel = cancel.clone();
    callbacks.push_negotiation(move |_| {
        check_git_cancelled(push_negotiation_cancel.as_ref())
            .map_err(|error| git2::Error::from_str(&error))
    });
    callbacks.credentials(move |url, username_from_url, allowed| {
        if let Some(credentials) = credentials.as_ref() {
            if allowed.is_user_pass_plaintext() {
                return git2::Cred::userpass_plaintext(
                    credentials.username.trim(),
                    credentials.password_or_token.trim(),
                );
            }
            if allowed.is_username() {
                return git2::Cred::username(credentials.username.trim());
            }
        }
        if allowed.is_ssh_key() || allowed.is_ssh_memory() {
            let username = username_from_url.unwrap_or("git");
            if let Ok(cred) = git2::Cred::ssh_key_from_agent(username) {
                return Ok(cred);
            }
            for key in default_ssh_key_paths() {
                if key.exists()
                    && let Ok(cred) = git2::Cred::ssh_key(username, None, &key, None)
                {
                    return Ok(cred);
                }
            }
        }
        if allowed.is_user_pass_plaintext()
            && let Ok(config) = git2::Config::open_default()
            && let Some((username, password)) = git2::CredentialHelper::new(url)
                .config(&config)
                .username(username_from_url)
                .execute()
        {
            return git2::Cred::userpass_plaintext(&username, &password);
        }
        if allowed.is_username() {
            return git2::Cred::username(username_from_url.unwrap_or("git"));
        }
        if allowed.is_default() {
            return git2::Cred::default();
        }
        Err(git2::Error::from_str(
            "No compatible Git credential was found.",
        ))
    });
    callbacks
}

fn check_git_cancelled(cancel: Option<&GitCancelToken>) -> Result<(), String> {
    if is_git_cancelled(cancel) {
        Err("Git operation cancelled.".to_string())
    } else {
        Ok(())
    }
}

fn is_git_cancelled(cancel: Option<&GitCancelToken>) -> bool {
    cancel
        .map(|token| token.load(Ordering::Relaxed))
        .unwrap_or(false)
}

fn git_error_message(error: git2::Error) -> String {
    if error.code() == git2::ErrorCode::User {
        "Git operation cancelled.".to_string()
    } else {
        normalize_git_error_message(error.message())
    }
}

fn normalize_git_error_message(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("unstaged changes exist in workdir")
        || lower.contains("uncommitted changes exist in index")
        || lower.contains("would be overwritten by checkout")
        || lower.contains("local changes would be overwritten")
    {
        return "Pull requires a clean working tree. Commit, stash, or discard local changes, then try again.".to_string();
    }
    if lower.contains("cannot push because a reference that you are trying to update on the remote contains commits that are not present locally") {
        return "Push rejected because the remote branch has commits that are not present locally. Pull or sync first, then push again.".to_string();
    }
    message.to_string()
}

fn default_ssh_key_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return Vec::new();
    };
    let ssh = PathBuf::from(home).join(".ssh");
    ["id_ed25519", "id_rsa", "id_ecdsa"]
        .into_iter()
        .map(|name| ssh.join(name))
        .collect()
}

#[cfg(test)]
mod remote_operation_tests {
    use super::{
        check_git_cancelled, normalize_git_error_message, system_git_command,
        system_git_output_message,
    };
    use std::{
        sync::{
            Arc,
            atomic::AtomicBool,
        },
    };

    #[test]
    fn normalizes_pull_dirty_worktree_errors() {
        assert_eq!(
            normalize_git_error_message("unstaged changes exist in workdir"),
            "Pull requires a clean working tree. Commit, stash, or discard local changes, then try again."
        );
        assert_eq!(
            normalize_git_error_message("Your local changes would be overwritten by checkout"),
            "Pull requires a clean working tree. Commit, stash, or discard local changes, then try again."
        );
    }

    #[test]
    fn cancelled_token_stops_remote_git_before_spawn() {
        let cancelled = Arc::new(AtomicBool::new(true));

        assert_eq!(
            check_git_cancelled(Some(&cancelled)).expect_err("cancelled"),
            "Git operation cancelled."
        );
    }

    #[test]
    fn system_git_error_prefers_stderr() {
        let output = system_git_command()
            .args(["not-a-codux-command"])
            .output()
            .expect("run git");

        let message = system_git_output_message(&output);

        assert!(message.contains("not-a-codux-command"));
    }
}
