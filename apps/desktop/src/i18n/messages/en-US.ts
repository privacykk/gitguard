import type { MessageSchema } from "./zh-CN";

const enUS = {
  app: {
    title: "GitGuard",
    subtitle: "Local-first Git push protection",
    loading: "Loading…",
    desktopUnavailable:
      "No desktop backend detected: this page is read-only and pushes are still rejected (fail-closed).",
    readOnly: "Read-only",
    productStage: "Push confirmation MVP",
  },
  nav: {
    label: "Main navigation",
    dashboard: "Overview",
    identities: "Identities",
    repositories: "Repositories",
    policies: "Policies",
    approval: "Approvals",
    audit: "Audit log",
    settings: "Settings",
    pending: "Pending",
    settingsShortcut: "Open settings ({shortcut})",
    collapseSidebar: "Collapse sidebar",
    expandSidebar: "Expand sidebar",
  },
  common: {
    enabled: "Enabled",
    disabled: "Disabled",
    planned: "Planned",
    loading: "Loading…",
    save: "Save",
    saved: "Saved",
    back: "Back",
    manage: "Manage",
    none: "None",
    ok: "OK",
    error: "Error",
    unknown: "Unknown",
    loadFailed:
      "Settings have not loaded, so nothing can be changed and protection state cannot be determined.",
    count: "{count}",
  },
  hookStatus: {
    label: "Hook status",
    healthy: "Healthy",
    missing: "Not installed",
    modified: "Modified",
    conflict: "Conflicts with another hook",
    unknown: "Unknown (treated as fail-closed)",
  },
  dashboard: {
    title: "Overview",
    subtitle:
      "See the protection scope, hook status and pending requests at a glance.",
    statusRunning: "Protection is running",
    statusAttention: "Issues need attention",
    statusIdle: "No push protection is enabled yet",
    statusReadOnly: "Read-only",
    stats: {
      repositories: "Registered repositories",
      protectedRepositories: "Protection enabled",
      pending: "Pending requests",
      unregistered: "Unregistered repos",
    },
    pendingOne: "A push request is waiting for confirmation",
    pendingNone: "No pending requests right now",
    healthTitle: "Runtime status",
    configHealthy: "The configuration file is readable",
    configError: "The configuration file cannot be read",
    configUnknown: "The configuration file state is unknown",
    ipcHealthy: "The desktop IPC service is running",
    ipcError: "The desktop IPC service is not running",
    ipcUnknown: "The desktop IPC service state is unknown",
    hooksTitle: "Hook checks",
    hooksHealthy: "Every registered repository has a healthy hook",
    hooksIssues: "{count} repositories need hook attention",
    hooksUnknown:
      "The settings are not loaded, so hook state cannot be determined",
    hooksEmpty: "No repositories yet; hooks are installed when you add one.",
    repositoriesTitle: "Registered repositories",
    quickTitle: "Next steps",
    addRepository: "Add a repository",
    reviewPolicies: "Review policies",
    reviewApproval: "See the approval flow",
    readOnlyHint:
      "No desktop backend is available, so nothing on this page can be changed.",
  },
  identities: {
    title: "Identities",
    subtitle:
      "Manage Git identities used for pushes and the boundary with remote authentication.",
    unavailableTitle: "Identity management is not available yet",
    unavailableBody:
      "GitGuard does not create, store or switch Git identities, and it never reads system credentials. Identity management is on the roadmap and is not implemented.",
    boundaryTitle: "Security boundary",
    boundaryBody:
      "GitGuard does not treat user.name / user.email as a remote authentication identity, and it never stores tokens, private keys or URLs containing passwords.",
    plannedTitle: "Planned capabilities",
    plannedCreate: "Create and edit Git identities",
    plannedCreateBody:
      "Maintain an identity name and commit email for different repositories.",
    plannedBind: "Bind an identity to a repository",
    plannedBindBody:
      "Choose which identity each registered repository pushes with.",
    plannedPriority: "Identity precedence",
    plannedPriorityBody:
      "Fall back in an explicit order when a repository has no binding.",
  },
  repositories: {
    title: "Repositories",
    subtitle:
      "Register local repositories and install the pre-push hook: target refs and commits are shown first, and the original git push only continues after you confirm.",
    listTitle: "Protected repositories",
    empty: "No repositories registered yet.",
    addTitle: "Add a repository",
    addLabel: "Repository path",
    addPlaceholder: "/absolute/path/to/repository",
    addButton: "Add repository",
    adding: "Adding…",
    browseButton: "Choose folder…",
    browsing: "Opening…",
    addWithProtection: "Enable push protection after adding",
    toggleLabel: "Push protection for {path}",
    note: "An existing pre-push hook is first backed up as pre-push.gitguard-backup before the chained script is written. Adding an already registered repository only refreshes its hook script; the protection switch stays as configured in the list.",
  },
  policies: {
    title: "Policies",
    subtitle:
      "Only the confirmation scope is configured here. Allow decisions are always made in Rust; UI state is not a security control.",
    globalTitle: "Global default",
    globalHint:
      "Affects only repositories not registered under Repositories; each registered repository follows its own switch.",
    unregisteredProtection:
      "Also confirm pushes from unregistered repositories",
    orderTitle: "Evaluation order",
    orderStep1:
      "Registered repository: only its own push protection switch counts.",
    orderStep2: "Unregistered repository: the global default above applies.",
    orderStep3:
      "Unreadable or undecidable configuration: the push is rejected (fail-closed).",
    overridesTitle: "Per-repository switches",
    overridesEmpty: "No repositories yet, so there is nothing to summarize.",
    overridesHint:
      "Switches are changed under Repositories; this list is read-only.",
    channelTitle: "Confirmation channel",
    channelBody:
      "Pushes in a terminal are confirmed by the CLI in that terminal; without a controlling terminal (graphical clients) the desktop app asks instead. The channel is chosen from the environment and cannot be configured.",
    timeoutHint: "The confirmation timeout is changed under Settings.",
    plannedTitle: "Planned policy capabilities",
    plannedPriority: "Policy precedence",
    plannedPriorityBody:
      "Evaluate global, repository and identity rules in a fixed order.",
    plannedBinding: "Identity and remote binding",
    plannedBindingBody:
      "Decide by remote URL rather than only by local repository.",
    openSettings: "Open settings",
    goToRepositories: "Go to repositories",
  },
  approval: {
    title: "Approvals",
    subtitle:
      "Before confirming, review the repository, remote, target refs and commits.",
    pendingTitle: "A push request is waiting",
    pendingBody:
      "Only one request is kept at a time; when a new one arrives, the previous request is cancelled.",
    goToApproval: "Go to the request",
    emptyTitle: "No push is waiting for confirmation",
    emptyBody:
      "When a graphical client (SourceTree, Fork, VS Code, …) triggers a push, the request appears here. You must type yes before the countdown ends.",
    flowTitle: "How confirmation works",
    flowStep1:
      "The hook intercepts git push and reads the target refs and commits.",
    flowStep2:
      "The desktop app shows the request and waits for yes before the countdown ends.",
    flowStep3:
      "After you confirm, refs are verified again; only then is the push allowed, and the original git push still performs it.",
    flowTerminal:
      "Pushes started in a terminal are confirmed there and never appear here.",
    readOnly:
      "Without a desktop backend, confirmation requests cannot be received.",
  },
  audit: {
    title: "Audit log",
    subtitle:
      "Record push confirmations and configuration changes for later review.",
    unavailableTitle: "The audit log is not available yet",
    unavailableBody:
      "GitGuard only writes errors to stderr; it does not persist an audit trail and never uploads data. The audit log is on the roadmap and is not implemented.",
    plannedTitle: "Planned capabilities",
    plannedPush: "Push event records",
    plannedPushBody:
      "Record the request, the confirmed or rejected result, and the time.",
    plannedConfig: "Configuration change records",
    plannedConfigBody:
      "Record switch, timeout and repository registration changes.",
    plannedExport: "Redaction and export",
    plannedExportBody:
      "Filter credentials and control characters before export so logs do not become a new leak surface.",
    boundary: "Nothing shown in the current UI constitutes an audit record.",
  },
  settings: {
    title: "Settings",
    subtitle:
      "Application-level preferences: interface language, confirmation timeout and shortcuts.",
    backHint: "Press Esc or choose Back to return to the previous page.",
    languageTitle: "Interface language",
    languageHint:
      "Takes effect immediately and is remembered; the native menu follows.",
    behaviorTitle: "Confirmation behavior",
    timeoutLabel: "Confirmation timeout (seconds)",
    timeoutHint:
      "1–3600 seconds; both terminal and desktop confirmation use this limit.",
    timeoutSave: "Save",
    timeoutSaved: "Saved",
    shortcutsTitle: "Keyboard shortcuts",
    shortcutSettings: "Open settings",
    shortcutClose: "Close settings",
    shortcutConfirm:
      "Confirming a push always requires typing yes; there is no shortcut",
    configTitle: "Configuration file",
    configPathLabel: "Path",
    configPathHint:
      "Read-only. Change settings through the UI; GitGuard never overwrites a configuration file it cannot read.",
    aboutTitle: "About",
    aboutBody:
      "GitGuard is a local-first Git push protection tool. This is the push confirmation MVP and supports macOS / Linux only.",
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
