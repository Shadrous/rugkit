# Implicit Spring Motion

Motion in rugkit is implicit by default. Whenever a reactive property changes — frame size, position, corner radius, background colour, elevation, scale, lift, opacity or focus ring — the retained node retargets its `Animated<T>` spring and **preserves its velocity** when interrupted.

## Spring Presets

- `Spring::BOUNCY` — playful geometry spring (`0.45s`, damping `0.62`), used for layout, size, elevation and scale.
- `Spring::SNAPPY` — crisp colour and opacity spring.

## Opting Out & Custom Springs

- `.animation(Spring::new(0.3, 0.8))` sets a custom spring for a widget's geometry.
- `.no_animation()` switches a widget to instant updates.

## Reduce Motion

rugkit polls the macOS Accessibility **Reduce Motion** preference (`NSWorkspace::accessibilityDisplayShouldReduceMotion`). When enabled, geometry changes snap immediately while colour transitions become short, gentle fades.

## Slow-Motion Debugging

Press `⌘⌥I` to open the in-app inspector and press `S` to cycle animation speed (`1.0×` → `0.25×` → `0.1×`) to inspect spring curves and interruptions frame by frame.
