# Quick settings refinement

Verified 2026-10-23 with Vigil's synthetic preview in isolated Niri/Weston at
1280×800. Screenshots crop only the panel and contain fixture data.

- `overview.png`: header, suggestions, enabled/off controls and slim level tracks.
- `overview-scrolled.png`: keyboard navigation scrolls the overview on a short
  output; the entire overview footer focus path was not confirmed.
- `power.png`: selected profile, neutral metric tiles and fully visible footer.

UI theme and Vigil desktop contracts, shell syntax and diff checks pass. No CSS
parser errors appeared. Nested software EGL/portal warnings remain environment
limitations. The live Vigil service received SIGHUP to apply the stylesheet.

All changes are scoped desktop CSS; application defaults and control actions are
unchanged. Scale widgets retain a 28 px interaction height around 6 px tracks.
