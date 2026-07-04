# Friendly UI Typography Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace monospace text across the active Wayland UI with Cantarell while preserving Waybar icon glyphs through a dedicated Symbols Nerd Font fallback.

**Architecture:** Keep typography in each component's existing source configuration and Matugen template; generated theme outputs remain build artifacts. Extend the shared shell contract test to encode the font roles before changing the sources, then render and smoke-test the affected interfaces.

**Tech Stack:** Bash, GTK CSS, SCSS, Dunst INI, Hyprlock configuration, Matugen, Fontconfig.

---

### Task 1: Define the typography contract

**Files:**
- Modify: `tests/ui_theme_contract_test.sh`

- [ ] **Step 1: Add helpers and failing font assertions**

Add this helper after `assert_count`:

```bash
assert_not_contains() {
  local file="$1"
  local rejected="$2"

  if grep -Fq -- "$rejected" "$ROOT_DIR/$file"; then
    printf 'Expected %s not to contain:\n  %s\n' "$file" "$rejected" >&2
    return 1
  fi
}
```

Add these assertions before the final success message:

```bash
assert_contains matugen/templates/luma.css \
  'font-family: "Cantarell", sans-serif;'
assert_contains waybar/style.css \
  'font-family: "Cantarell", "Symbols Nerd Font", sans-serif;'
assert_contains eww/eww.scss \
  'font-family: "Cantarell", sans-serif;'
assert_contains matugen/templates/dunstrc \
  'font = Cantarell 11'
assert_count hypr/hyprlock.conf 5 \
  'font_family = Cantarell'
assert_contains hypr/hyprlock.conf \
  'font_family = Cantarell ExtraBold'
assert_contains matugen/templates/greetd.css \
  'font-family: "Cantarell", sans-serif;'
assert_contains matugen/UI_STYLE.md \
  'Typography: Cantarell for interface text, with Symbols Nerd Font limited to icon fallback'

assert_not_contains matugen/templates/luma.css 'JetBrains Mono'
assert_not_contains waybar/style.css 'JetBrainsMono Nerd Font'
assert_not_contains eww/eww.scss 'JetBrains Mono'
assert_not_contains matugen/templates/dunstrc 'JetBrainsMono Nerd Font'
assert_not_contains hypr/hyprlock.conf 'JetBrains Mono'
assert_not_contains matugen/templates/greetd.css 'JetBrains Mono'
```

- [ ] **Step 2: Run the contract test and confirm the expected failure**

Run:

```bash
bash tests/ui_theme_contract_test.sh
```

Expected: FAIL because `matugen/templates/luma.css` does not yet contain the Cantarell font stack.

- [ ] **Step 3: Commit the failing contract**

```bash
git add tests/ui_theme_contract_test.sh
git commit -m "test(ui): define friendly typography contract"
```

### Task 2: Apply role-based fonts to active Wayland UI sources

**Files:**
- Modify: `matugen/templates/luma.css`
- Modify: `waybar/style.css`
- Modify: `eww/eww.scss`
- Modify: `matugen/templates/dunstrc`
- Modify: `hypr/hyprlock.conf`
- Modify: `matugen/templates/greetd.css`
- Modify: `matugen/UI_STYLE.md`

- [ ] **Step 1: Replace the Luma, Eww, Dunst, and greetd text families**

Use these exact declarations in their existing global typography locations:

```css
/* matugen/templates/luma.css, eww/eww.scss, matugen/templates/greetd.css */
font-family: "Cantarell", sans-serif;
```

```ini
# matugen/templates/dunstrc
font = Cantarell 11
```

- [ ] **Step 2: Give Waybar a proportional text family with an icon fallback**

Replace the existing global Waybar `font-family` with:

```css
font-family: "Cantarell", "Symbols Nerd Font", sans-serif;
```

Keep `font-size: 12px`, `font-weight: 500`, and all spacing unchanged.

- [ ] **Step 3: Replace every Hyprlock label family while preserving clock weight**

Use `font_family = Cantarell` for the date, prompt, calendar, and media labels. Use this only for the large clock label:

```ini
font_family = Cantarell ExtraBold
```

- [ ] **Step 4: Update the shared UI typography documentation**

Replace the typography bullet in `matugen/UI_STYLE.md` with:

```markdown
- Typography: Cantarell for interface text, with Symbols Nerd Font limited to icon fallback; use `on_surface_variant` for supporting text.
```

- [ ] **Step 5: Run the contract and source checks**

Run:

```bash
bash tests/ui_theme_contract_test.sh
git diff --check
```

Expected: `Balanced Glass UI contract verified.` and exit status 0.

- [ ] **Step 6: Commit the implementation**

```bash
git add matugen/templates/luma.css waybar/style.css eww/eww.scss \
  matugen/templates/dunstrc hypr/hyprlock.conf \
  matugen/templates/greetd.css matugen/UI_STYLE.md
git commit -m "feat(ui): use friendly proportional typography"
```

### Task 3: Render and verify typography live

**Files:**
- Generated, temporary: `/tmp/friendly-ui-typography/Luma/theme.css`
- Generated, temporary: `/tmp/friendly-ui-typography/waybar/colors.css`
- Generated, temporary: `/tmp/friendly-ui-typography/eww/colors.scss`
- Generated, temporary: `/tmp/friendly-ui-typography/dunst/dunstrc`

- [ ] **Step 1: Confirm the required fonts resolve through Fontconfig**

Run:

```bash
fc-match Cantarell
fc-match "Cantarell ExtraBold"
fc-match "Symbols Nerd Font"
```

Expected: each command resolves to its requested installed family.

- [ ] **Step 2: Render the current `#8394a8` palette through temporary outputs**

Create the output directories:

```bash
mkdir -p /tmp/friendly-ui-typography/{Luma,waybar,eww,dunst,greetd}
cp waybar/style.css /tmp/friendly-ui-typography/waybar/style.css
cp eww/eww.scss /tmp/friendly-ui-typography/eww/eww.scss
```

Create `/tmp/friendly-ui-typography-matugen.toml` with this exact content:

```toml
[config]

[templates.luma]
input_path = '/home/robin/.dotfiles/.worktrees/friendly-ui-typography/matugen/templates/luma.css'
output_path = '/tmp/friendly-ui-typography/Luma/theme.css'

[templates.waybar]
input_path = '/home/robin/.dotfiles/.worktrees/friendly-ui-typography/matugen/templates/waybar.css'
output_path = '/tmp/friendly-ui-typography/waybar/colors.css'

[templates.eww]
input_path = '/home/robin/.dotfiles/.worktrees/friendly-ui-typography/matugen/templates/eww.scss'
output_path = '/tmp/friendly-ui-typography/eww/colors.scss'

[templates.dunst]
input_path = '/home/robin/.dotfiles/.worktrees/friendly-ui-typography/matugen/templates/dunstrc'
output_path = '/tmp/friendly-ui-typography/dunst/dunstrc'

[templates.greetd]
input_path = '/home/robin/.dotfiles/.worktrees/friendly-ui-typography/matugen/templates/greetd.css'
output_path = '/tmp/friendly-ui-typography/greetd/style.css'
```

Then run:

```bash
matugen color hex '#8394a8' --config /tmp/friendly-ui-typography-matugen.toml
```

Expected: exit status 0 and rendered files containing `Cantarell`.

- [ ] **Step 3: Validate rendered styles and repository configuration**

The temporary Eww source and colors now share a directory, so the import resolves without modifying tracked sources. Run:

```bash
sassc /tmp/friendly-ui-typography/eww.scss /tmp/friendly-ui-typography/eww.css
Hyprland --verify-config --config hypr/hyprland-config/base.conf
niri validate -c niri/config.kdl
jq empty waybar/config
bash -n setup.sh scripts/*.sh waybar/scripts/*.sh \
  hypr/scripts/modes/*.sh bspwm/modes/*.sh bspwm/helpers/*
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 4: Smoke-test Waybar and Luma using temporary rendered files**

Make the normal Luma settings available in the temporary configuration root:

```bash
ln -sf /home/robin/.config/Luma/config.json \
  /tmp/friendly-ui-typography/Luma/config.json
```

Run Waybar and Luma for four seconds and reject parser, theme, or font-loading errors:

```bash
set +e
waybar_output=$(timeout --signal=INT 4s waybar \
  -c waybar/config \
  -s /tmp/friendly-ui-typography/waybar/style.css 2>&1)
waybar_status=$?
luma_output=$(timeout --signal=INT 4s env \
  XDG_CONFIG_HOME=/tmp/friendly-ui-typography \
  XDG_SESSION_TYPE=wayland \
  GDK_BACKEND=wayland \
  tools/launcher/target/release/Luma \
  --query friendlytypographypreviewzz 2>&1)
luma_status=$?
set -e

printf '%s\n%s\n' "$waybar_output" "$luma_output"
! printf '%s\n%s\n' "$waybar_output" "$luma_output" |
  grep -Eiq 'css.*(error|failed)|parser error|theme.*failed|font.*failed'
test "$waybar_status" -eq 124
test "$luma_status" -eq 124
```

Expected: both processes time out with status 124 after starting successfully; Waybar reports configured bars for the active outputs.

- [ ] **Step 5: Inspect the live UI**

Open Luma over the wallpaper and start a temporary Waybar instance. Capture screenshots and confirm:

- Luma search and result text are proportional Cantarell.
- Waybar labels are Cantarell.
- Workspace, status, and module icons still render through Symbols Nerd Font.
- Text remains readable and no module dimensions visibly regress.

- [ ] **Step 6: Run the final regression suite**

```bash
bash tests/ui_theme_contract_test.sh
cargo fmt --manifest-path tools/launcher/Cargo.toml --check
cargo test --manifest-path tools/launcher/Cargo.toml --quiet
cargo build --manifest-path tools/launcher/Cargo.toml --release --quiet
git diff --check
```

Expected: the typography contract passes, all 141 launcher tests pass, the release build succeeds, and the worktree is clean.

- [ ] **Step 7: Integrate and apply locally if local merge is selected**

Fast-forward the verified branch into `master`, regenerate the live Luma, Waybar, Eww, and Dunst outputs from the source templates, then run:

```bash
hyprctl reload
pkill -USR2 -x waybar
hyprctl configerrors
```

Expected: Hyprland reloads, one Waybar process exposes bars on all active outputs, and `hyprctl configerrors` prints no errors.
