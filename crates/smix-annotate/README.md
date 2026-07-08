# smix-annotate

Screenshot annotation primitives + PNG compression for [smix](https://github.com/goliajp/smix).

## Scope

- 5 primitives: **circle**, **arrow**, **text**, **box**, **line**
- 3 compression presets: **fast**, **balanced** (default), **aggressive** (via oxipng)
- Named + hex + rgba color palette, plus semantic colors (`EXPECTED`, `ACTUAL`, `HINT`, `ERROR`, `SUCCESS`)
- Position: absolute pixel + normalized 0..1
- Text rendering via `ab_glyph` (font supplied by caller)

## Library usage

```rust
use smix_annotate::{Annotator, Annotation, Color, Position, Compression};

let png = std::fs::read("input.png")?;
let out = Annotator::new(&png)?
    .add(Annotation::circle(Position::pixel(100, 100))
        .color(Color::RED)
        .radius(40))
    .compression(Compression::BALANCED)
    .render()?;
std::fs::write("output.png", out)?;
```

## Non-goals

- Selector-relative position resolution (smix-adapter-maestro runtime handles it)
- Auto-annotate for `--debug-output` fail-step screenshots
