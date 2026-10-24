import type { MessageSchema } from "./zh-CN";

const enUS = {
  app: {
    title: "GitGuard",
    subtitle: "Local-first Git push protection",
    loading: "Loading…",
    desktopUnavailable:
      "No desktop backend detected: this page is read-only and pushes are still rejected (fail-closed).",
  },
  language: {
    label: "Interface language",
  },
  push: {
    title: "Confirm push",
    intro:
      "The following changes are about to be pushed. Review them carefully.",
    repository: "Repository",
    remote: "Remote",
    remoteUrl: "Remote URL",
    targetRef: "Target ref",
    sourceRef: "Local ref",
    commits: "Commits to push",
    noCommits: "No new commits",
    kind: {
      create: "This creates a new remote branch",
      update: "This updates a remote branch",
      delete: "This deletes a remote branch",
    },
    yesLabel: 'Type "yes" to perform this push',
    inputPlaceholder: "yes",
    confirmButton: "Confirm this push",
    cancelButton: "Cancel",
    executing: "Confirming…",
    remaining: "Time left to confirm: {seconds}s",
    expired: "Confirmation timed out; the push was rejected.",
    confirmed:
      "Confirmed. The original git push will continue; check the push result in your terminal or Git client.",
  },
  commit: {
    sha: "SHA",
    author: "Author",
    time: "Time",
    expand: "Show all {count}",
    collapse: "Collapse",
    empty: "No commits to display.",
  },
  settings: {
    title: "Repositories",
    intro:
      "Adding a repository installs the GitGuard pre-push hook: the target refs and commits are shown first, and the original git push only continues after you confirm.",
    repositories: "Protected repositories",
    empty: "No repositories added yet.",
    addLabel: "Repository path",
    addPlaceholder: "/absolute/path/to/repository",
    addButton: "Add repository",
    adding: "Adding…",
    browseButton: "Choose folder…",
    browsing: "Opening…",
    addWithProtection: "Enable push protection after adding",
    toggleLabel: "Push protection for {path}",
    protection: "Push protection",
    enabled: "Enabled",
    disabled: "Disabled",
    hookStatusLabel: "Hook status",
    hookStatus: {
      healthy: "Healthy",
      missing: "Not installed",
      modified: "Modified",
      conflict: "Conflicts with another hook",
      unknown: "Unknown (treated as fail-closed)",
    },
    timeoutLabel: "Confirmation timeout (seconds)",
    timeoutSave: "Save",
    timeoutSaved: "Saved",
    defaultProtection: "New repositories enable push protection by default",
  },
  errors: {
    unknown:
      "The operation failed for an unrecognized reason; pushes remain rejected.",
    configuration_unreadable:
      "The configuration file cannot be read, so writing has stopped to avoid overwriting it.",
    configuration_write_failed: "The configuration file could not be written.",
    repository_not_found: "No configuration entry for that repository.",
    hook_install_failed: "Installing the hook failed.",
    cli_binary_not_found:
      "The gitguard CLI could not be found, so the hook was not written.",
    not_a_repository: "The selected path is not a Git repository.",
    path_not_absolute:
      "Enter an absolute repository path (the Choose folder button fills one in); a relative path cannot be resolved.",
    no_pending_request: "That confirmation request timed out or was cancelled.",
    request_invalid:
      "The repository changed while confirming; the push was dropped.",
    invalid_timeout:
      "The timeout must be a whole number of seconds from 1 to 3600.",
    ipc_unavailable:
      "The desktop IPC service is not running, so hooks cannot connect.",
    internal_error: "Internal desktop error.",
  },
} satisfies MessageSchema;

export default enUS;
