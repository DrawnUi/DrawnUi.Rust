# drawnui-add3d

An add-on for [DrawnUI for Rust](https://crates.io/crates/drawnui): real 3D inside a DrawnUI screen, next to its
buttons, text and lists.

- `SkiaModel3D`: one glTF / GLB scene with its skeleton animations; play, play once and back, hold, cross-fade;
  nodes and materials by name; taps that tell what was hit; drag to turn.
- `SkiaScene3D`: one 3D layer over a 2D game field: many objects placed in points like sprites, each with its own
  animations, drawn in one pass, each model loaded once.
- `Model3D` / `Mesh3D`: shapes made in code (discs, spheres, rings, boxes, turned profiles, any 2D path made 3D).
- Images on materials: `set_material_image` puts any image (a PNG or JPEG file, a photo, a pattern drawn with Skia)
  on a part of a model or a shape made in code, and switches it live (`Texture3D`).
- Looks: shaded, cartoon (two tones) or flat, with an optional outline; smooth edges with `antialias(true)`.

```toml
[dependencies]
drawnui = "0.1.0-preview"
drawnui-add3d = "0.1.0-preview"
```

```rust
use drawnui::prelude::*;
use drawnui_add3d::SkiaModel3D;

SkiaModel3D::new(include_bytes!("assets/boss.glb"))
    .animation("Idle")
    .on_tapped_3d(|me, _app: &mut App, _cx, _hit| { me.play_once("Hit"); })
```

Draws in the browser (WebGL2) for now; other platforms draw nothing yet. Examples: `examples/model3d` (a boss fight)
and `examples/gallery3d` (shapes made in code, images on a box and a ball, an avatar, a reward chest, a field of mobs) in the repository.
MIT license.
