## Title
`Input: expose modifier changes while pointer is stationary for hover affordance`

## Body
In rune-term (a terminal UI built on Rune), links activate on modifier+click (Cmd on macOS, Ctrl elsewhere) and underline on hover. To show clear affordance, the UI needs to react when the activation modifier is pressed or released while the pointer is already hovering a link, without requiring pointer movement. Rune could expose modifier-state changes to the app/hovered element while the pointer is stationary (or an equivalent hook), and consider cursor affordance for “modifier+click activates this target”. See also #106 for focus traversal observation.
