# Changelog

## Unreleased

### Added

- `SkiaModel3D`: a glTF / GLB scene drawn in 3D inside a DrawnUI screen (depth buffer, a light, base colors and
  color textures, skeleton animations). `play` loops, `play_once` plays once and blends back to the loop, `hold`
  stays on the last frame, `play_layer` runs another animation at the same time, moves cross-fade
  (`crossfade_ms`). Nodes by name (`set_node_visible` / `_translation` / `_rotation` / `_scale`, `reset_node`),
  materials by name (`set_material_color`, `set_material_visible`). `on_animation_ended`, `on_tapped_3d`. Drag to
  turn, `auto_rotate`, `zoom`, `set_view_angles`.
- `SkiaScene3D`: a 3D layer over a 2D game field. Many objects, each a model with its own place, size, facing and
  animations, drawn in one pass; placed in the layer's points like sprites (`add`, `set_position`, `set_facing`,
  `play` / `play_once` / `hold`, `set_material_color`), lower ones in front, `tilt`. `on_tapped_3d` tells which
  object was hit. A model is read and uploaded once, however many objects and controls show it.
- `Model3D` / `Mesh3D`: shapes made in code (`cylinder`, `sphere`, `torus`, `cuboid`, `lathe`, `extrude` of any
  2D path) put together as named, colored parts; `SkiaModel3D::from_model`, `SkiaScene3D::model_from`.
- `look(Look::Lit | Look::Toon | Look::Flat)` and `outline(width, color)`.
- `SkiaModel3D` for real screens: `from_url` / `set_source` load a model by url with `load_state` and
  `on_load_state` (Loading, Ready, Failed); `screen_position` tells where a point of the scene is on
  screen and whether the model covers it (anchored labels, pins hidden on the far side);
  `on_view_changed` runs once per frame in which the view moved; `pinch_zoom` (two fingers and the
  mouse wheel) within `min_zoom` / `max_zoom`; `reset_view` glides back to the view the app set,
  `reset_on_double_tap` does it on two taps; `lock_pitch` turns around the vertical axis only and
  leaves vertical drags to the scroll around it; `tilt_to_pointer` leans the model toward the mouse
  or finger and back home when it leaves. A model or layer with nothing on screen stands still and
  asks for no frames until it is scrolled back into view.
- The shaded look lights with a key light, a soft fill from the other side and the sky, and gives
  smooth materials a highlight by their glTF roughness; `set_material_roughness` changes it per
  material (leather shines, linen does not).
- `antialias(true)` / `set_antialias` on `SkiaModel3D` and `SkiaScene3D`: smooth edges with 4x multisampling (or
  the most the device offers). Off by default; on uses more GPU memory per pixel of the control, the same CPU time.
- Images on materials: `Texture3D` (from PNG / JPEG / WebP bytes, or any Skia image in memory), put on a material by
  name with `set_material_image` (`SkiaModel3D`, and per object on `SkiaScene3D`) or `material_image` when built;
  switched live, uploaded once however many materials show it. Shapes made in code carry texture coordinates (a
  box shows the image on each face, a sphere wraps it around, an extruded path shows it on its front), and spheres
  and rings are shaded round.
