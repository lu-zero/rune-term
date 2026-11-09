## Title
`Input: expose modifier changes while pointer is stationary for hover affordance`

## Body
In rune-term (a terminal UI built on Rune), links are activated with modifier+click (Cmd on macOS, Ctrl elsewhere) and are underlined on hover. To give clear affordance, the UI needs to react when the user presses/releases the activation modifier while the pointer is already hovering a link (refresh underline/cursor state) without requiring pointer movement.

**Request**
- Deliver modifier-state changes to the app/hovered element while the pointer is stationary (or provide an equivalent hook), so hover UI can update on modifier press/release.
- If appropriate, consider cursor affordance integration for “modifier+click activates this target” cases.

**Use case**
Terminal hyperlink hover: show/refresh link affordance and, where supported, pointer cursor feedback when the activation modifier is held over a detected URL/OSC 8 link.

**Related**
Element focus traversal observation for multi-pane focus sync is tracked separately in #106 (`El::on_focus_change`).
