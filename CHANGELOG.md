# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- `jung-core`: labels render on the default path. `Renderer::render` reads
  `text-field`, `text-size`, `text-font` and `text-color`, expands `{property}`
  tokens, deconflicts by priority derived from text size, places point labels
  straight and line labels along the geometry, and rasterizes through the TTF
  path. Fonts come from the caller via `Renderer::with_fonts`, and
  `text_layers_without_font` names the layers skipped when none is given.
  `Renderer::place_labels` returns the placements without drawing them.
  Polygons do not label, and `jung-vello` still carries geometry only.
- `jung-cli`: `--font` and `--font-family`, and a warning naming the text layers
  skipped for want of a font.

### Removed

- `jung-core`: `output` and `print_layout`. SVG export, print buffers and page
  furniture are gone. Fenestra prints PDFs with printpdf and tiny-skia, and
  ViewTopia prints client-side, so nothing consumed them. Test count 322 → 316.

### Changed

- README no longer claims ViewTopia styles maps through jung-wasm. It names the
  real callers (`jung-esri` in ptolemy, `jung-mvt` in terravista) and that
  `Renderer` draws points, lines and polygons. Test count 323 → 322.

## [0.2.0] - 2026-08-13

### Added

- `jung-mvt`: standalone Mapbox Vector Tile v2 decoder, geometry in tile units, `thiserror` its only dependency.
- `jung-esri`: translates Esri `drawingInfo` into Mapbox GL style layers plus a list of what could not be translated.
- `jung-esri`: `esriPMS` and `esriPFS` symbols with inline `imageData` become `icon-image` and `fill-pattern` layers plus a map of named data uri images to register.

### Fixed

- `jung-core`: multipolygon vector tile features decoded as one polygon whose extra parts were holes. Rings are now classified by winding as the spec requires, so a tile feature with several exterior rings decodes to `Geometry::MultiPolygon`. `jung-core::mvt` is now an adapter over `jung-mvt` that keeps its 0 to 1 normalisation and y flip.

## [0.1.0] - 2026-05-30

### Added

- Initial release.
