# svx-dll

Rust reader for Smithsonian Voyager scene documents (`.svx.json`), for Open BIM Suite and Open 3D
Viewer Studio. Pure Rust on bytes, so it also runs in the browser (WebAssembly).

| Crate | What it holds |
|---|---|
| `svx-model` | The neutral model: the scene's glTF models with their placements, and their annotations, in metres, Z up. |
| `svx-read` | The reader: the node tree with translation, rotation and scale; each model's units, pose and the glTF of the quality chosen among its derivatives (High first); the annotations with their titles and lead texts (Voyager 1.0 and 2.0 forms); the object's title and collection record. |

The glTF files themselves are not decoded here: the reader hands their bytes over, as read.

## Licence

LGPL-3.0-or-later. Test files: see `crates/svx-read/tests/data/NOTICE.md`.
