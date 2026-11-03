You are the commit-only phase BEFORE the automatic dotfiles improvement pass.

Inspect ALL pending changes, including staged, unstaged, untracked files and
initialized nested Git submodules. Group related changes by concern and commit
all changes with relevant Conventional Commit messages. Use separate commits
for unrelated changes. Commit submodules before their parent pointers.

Create these local automation commits with `git -c commit.gpgsign=false commit`
(including submodule commits), matching the wrapper's improvement commits.
Signing agents are unavailable inside the sandbox. Do not change persistent Git
configuration or request access to signing keys.

This invocation only records existing work. Do not implement improvements,
modify file contents, discard changes, amend existing commits, rewrite history,
change Git configuration, reload services, or send notifications. Do not include
ignored files or credentials. If something cannot be safely committed, report
failed and explain why; the improvement pass must not start with a dirty tree.
Honor repository validation requirements without auto-formatting or fixes.

For this automated phase, leave commits local and do not push. The caller owns
publication. This overrides any general commit/push instruction in AGENTS.md.

Return JSON matching the provided schema. Use status changed when commits were
created, no_change if already clean, or failed if incomplete. Include a short
summary, the relevant file paths, and validation commands actually performed.
Keep the title on one line.
