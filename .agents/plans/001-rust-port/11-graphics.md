# Issue 11 — `oh-render` graphics: render stages, draw methods, cameras

## Objective

Port the drawing layer: `HGGraphics.cpp` (render stages, per-stage shader
`RenderStates`, vertex flush order), the `HexagonGame::draw*` methods
(`drawText_*`, `drawKeyIcons`, `drawLevelInfo`, `drawParticles`,
`drawTrailParticles`, `drawSwapParticles`), `StyleData::drawBackground*`, the
`ssvs::Camera` analogue, and `FastVertexVectorTris` → GPU submission.

## Key decisions

- Reproduce the exact **RenderStage** order (from `hg::RenderStage`) and the
  per-stage fragment-shader binding.
- Cameras (`backgroundCamera`, `overlayCamera`): copy center/zoom/shake math from
  `HGGraphics.cpp` + `HexagonGame::updateCameraShake` and the pre-shake center logic.
- Text: mirrors `sf::Text` usage — copy padding/scaling/outline formulas exactly.

## Files

| File | Change |
|---|---|
| `crates/oh-render/src/{graphics,camera,batching,text_draw}.rs` | new |
| `crates/oh-render/tests/golden_frames.rs` | new — optional golden-frame harness |

## Steps

1. Port the batched vertex submission (`FastVertexVectorTris` → GPU buffer upload)
   with the same draw order.
2. Port `HGGraphics` stage rendering + shader-state selection.
3. Port each `draw*` method with exact formulas and layout constants.
4. Port `StyleData::drawBackground` / `drawBackgroundMenu` (tile/quad layout math).
5. Port the camera (world↔screen transform, zoom, shake).

## Verification

- Vertex-count/order tests for a fixed scene (assert the emitted vertex stream
  equals an expected sequence derived from the code).
- A golden-frame test (offscreen render of a fixed level/seed/frame) compared to a
  reference image generated once — mark as a coarse regression guard, not pixel law.
- `tools/gate.sh` green.

## Notes / risks

- Golden frames are sensitive to driver/AA differences; keep them coarse (layout +
  dominant colour) and document the tolerance; do not make them the sole gate.
