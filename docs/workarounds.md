# App Workarounds

This page documents known issues with specific applications and their workarounds using window rules.

## Firefox

Firefox creates temporary popup windows (dropdowns, autocomplete, etc.) that can cause flickering during window tiling. These popups have inconsistent `AXSubrole` values that sometimes bypass ignore rules.

**Workaround:**

```sh
# Ignore popup windows with AXUnknown subrole
yashiki rule-add --app-id org.mozilla.firefox --subrole AXUnknown ignore

# Ignore windows with empty titles (additional safety)
yashiki rule-add --app-id org.mozilla.firefox --title "" ignore
```

Add these rules to your `~/.config/yashiki/init` file.

## Debugging Window Issues

If you encounter similar issues with other applications, use debug logging to identify problematic windows:

```sh
RUST_LOG=yashiki=debug yashiki start
```

Look for lines like:
```
Discovered window: [12345] pid=1234 app='AppName' app_id=Some("com.example.app") title='' ax_id=None subrole=Some("AXUnknown")
```

Then create appropriate rules using `--app-id`, `--subrole`, `--ax-id`, or `--title` matchers.
