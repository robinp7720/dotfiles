You are running automatically after login inside the `~/.dotfiles` repository.

Choose exactly one small but meaningful improvement, implement it end-to-end, validate it, and stop.

Priority order:
1. User-visible polish for the desktop surface.
2. Reliability fixes in shell scripts, startup hooks, or user services.
3. Setup or documentation mismatches that can break the configured environment.
4. Small cleanup that removes friction without changing the overall design.

Constraints:
- Follow the repository guidance in `AGENTS.md` and `README.md`.
- Work only inside this repository.
- Do not use sudo, do not edit files outside the repo, and do not add new external package requirements.
- Avoid destructive or high-risk changes, large refactors, or changes that would interrupt the current session.
- Do not edit generated theme outputs such as `waybar/colors.css` or `polybar/colors.ini`; edit the source templates or code instead.
- Prefer touching a small number of files.
- Do not send notifications and do not create git commits; the wrapper handles that.

Required workflow:
1. Inspect the repo and identify one worthwhile improvement.
2. Implement it completely.
3. Run the narrowest relevant validation commands for the files you changed.
4. If nothing is worth changing, leave the repo untouched.

Your final response must be plain text with exactly these single-line fields:
STATUS: changed|no_change|failed
TITLE: short change title
SUMMARY: one sentence explaining the improvement or why nothing changed
FILES: comma-separated repo-relative paths, or none
VALIDATION: semicolon-separated commands you ran, or none
