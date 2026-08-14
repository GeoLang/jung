# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added

- `jung-mvt`: standalone Mapbox Vector Tile v2 decoder, geometry in tile units, `thiserror` its only dependency.
- `jung-esri`: translates Esri `drawingInfo` into Mapbox GL style layers plus a list of what could not be translated.
- `jung-esri`: `esriPMS` and `esriPFS` symbols with inline `imageData` become `icon-image` and `fill-pattern` layers plus a map of named data uri images to register.

### Fixed

- `jung-core`: multipolygon vector tile features decoded as one polygon whose extra parts were holes. Rings are now classified by winding as the spec requires, so a tile feature with several exterior rings decodes to `Geometry::MultiPolygon`. `jung-core::mvt` is now an adapter over `jung-mvt` that keeps its 0 to 1 normalisation and y flip.

## [0.1.0] - 2026-05-30

### Added

- Initial release.
