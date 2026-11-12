# WT Ballistic Visualizer

An offline map and ballistics tool for War Thunder ground battles. Pick a map and a tank, place a firing point **A** and a target **B**, and the tool tells you whether and how the shell gets there. It shows the exact gun elevation, the hold over the target, the flight path in 2D, in 3D and through a gunner's sight, and where the game's map border and red zones are.

There are two front ends that read the same data:

- **The native app** (`rust/`, Windows) is the main version. **Download `WT-Ballistic-Visualizer-Lite.exe` from [Releases](../../releases/latest)**: one file (about 560 MB) with all map and tank data inside. You don't need the game, a data folder or anything else installed. The Leopard 2A4 model stands in at A for every tank (see [Lite build](#lite-build)). A full build with each tank's own model and full-resolution ground colours (about 1.15 GB) can be built from source.
- **The web page** (`index.html`) is the original version. It runs in a Chromium browser or Firefox, opened next to the `data/` folder. It has everything except the gunner sight and placing A and B on objects (see [Web page](#web-page)).

## Quick start

Download `WT-Ballistic-Visualizer-Lite.exe` from [Releases](../../releases/latest) and run it (64-bit Windows 10 or 11). Windows may say "Windows protected your PC" because the file is not signed: click **More info**, then **Run anyway**.

1. Choose a **Level** (type to search) and a **Tank** and **Shell** in the sidebar.
2. **Left click** the map to place your tank at **A**. **Right click** to place the target at **B**.
3. Read the **Aim A → B** card: distance, whether the sight line and each firing solution are clear, gun elevation, and how far to aim over B.
4. Switch **View** to *3D orbit*, *3D aim* or *3D gunner sight* to see the shot from above or from behind the gun.

## Using it

### Views and controls

| | 2D map | 3D orbit | 3D aim / 3D gunner sight |
|---|---|---|---|
| Left click | place A | place A on the surface under the cursor | — |
| Right click | place B | place B | — |
| Drag | pan | rotate (right or shift drag pans) | — |
| Wheel | zoom around the cursor | zoom | aim: field of view; sight: magnification |
| Double click | — | centre on that spot | — |
| W / S (or ↑ / ↓) | drive A's tank forward / back | same | same (the camera rides along) |
| A / D (or ← / →) | steer A's tank left / right | same | same |

The **R** key, or the button at the top of the map, switches the map border on and off. **/** opens the level search. Keys act only while the map has the focus: click the map first, and typing in a search box or number field never drives the tank.

- **Driving A's tank:**
  - Hold **W** to drive forward at about 43 km/h and **S** to reverse at 18 km/h. **A** and **D** turn the hull at 45° per second, also on the spot.
  - The aim result, the profile, the arcs and the 3D views update as it moves, so you can drive to a spot and see at once whether the shot works from there.
  - The tank follows the ground. It pitches up and down with the slope under its front and back and rolls with the slope under its sides, on terrain, rocks and roofs alike. B's T-72A sits on the ground the same way.
  - Clicking a new A turns the tank to face B again. All views share A, B, the aim result and any sweep overlay.

- **A and B stand on whatever is there.** A tank placed on a rock (or a building roof or wall top) stands on its top, not on the ground inside it. The height used is the bare ground plus every solid object (see *Objects* under [Model and known limits](#model-and-known-limits)).
- **3D orbit:** the terrain with the game's ground colours, its objects and trees, water, zones, the 100 m grid, the shell arcs, and the tank models: your tank at A and a T-72A at B, facing each other.
- **3D aim:** the camera sits at A's gun (A's surface plus the gun height) and looks at B. A crosshair marks B. A tick marks the aim point of each firing solution, labelled with its hold in mils and metres.
- **3D gunner sight:** the same camera seen through a sight, at 1–24× magnification (8× to start; 1× is 70° vertical field of view).
  - The view is a round eyepiece with a crosshair, with a tick every 4 mil along the horizontal line, numbered every 8.
  - Below the centre is a **range ladder**: marks every 200 m, numbered in hundreds of metres (4 = 400 m, 12 = 1200 m). Put the mark for a target's range on the target and the gun is raised by the right amount, as with the range scale of a real tank sight. The ladder is worked out for the selected shell, so a slow shell has a long ladder and a fast APFSDS a short one.
  - The aim tick for B shows the exact solution.
- **3D terrain detail** sets how far full-resolution terrain reaches.

### Sidebar

- **Map:**
  - **Level** is a searchable list. Names are shown without `avg_`, with the map size.
  - **View** and **Game mode** (Arcade, Realistic or Simulator: the zones differ per mode).
  - **Mission** (the zones also differ per mission).
  - **Find highest point** marks the highest bare ground inside the map border and outside every red zone, for that mission and mode, and centres on it.
- **Layers:** switches for:
  - **Map border:** red stripes outside the battle area, where the countdown starts.
  - **Red zones:** the placed kill areas.
  - **Capture zones and spawns**, **Water** and the **100 m grid**.
  - **Objects**, **Trees and bushes**, **Tank models** and **Ground detail textures** in 3D.
  - Whether **trees and bushes block sight** (on by default) and **block shots** (off by default).
- **Gun:**
  - **Tank:** a searchable list of 1,219 vehicles.
  - **Shell:** shells marked *est. drag* use an estimated drag coefficient; see [Model and known limits](#model-and-known-limits).
  - **Pitch limits:** taken from the vehicle.
  - **Max range**, **gun height** and **target height**.
- **Aim A → B:**
  - Distance and slant distance, the height of A and B, azimuth, and the sight-line angle.
  - Whether the sight line is clear, and what blocks it (terrain, an object or trees).
  - For the low and high arc: whether it hits, the gun elevation in degrees and mils, the hold over B in mils and metres, the flight time and the impact speed.
  - **Shell arc in 3D** draws each arc with a see-through wall down to the ground, a dot every 0.5 s, the apex and the impact. Parts hidden from the camera are dashed.
  - The bottom panel shows the side profile from A to B: ground, objects, tree crowns, sight line and arcs.
- **Sweeps** (on all CPU cores):
  - **Reach from A:** every spot where a target can be hit from A.
  - **Hit target B:** every spot a shooter could hit B from. Green means the low arc works, orange means only the high arc works, blue means both.
  - **Visibility from A:** what A can see. Lines mark the horizons that cut the view: red for terrain, orange for objects, yellow for trees.
  - **Sweep performance** sets the number of rays, the step and the elevation resolution.
  - **Only battle area** clips the results to the battle area. Hit target B also leaves out the red zones, since a shooter can't hold a position there. Reach does not: each team's red zones apply only to the other team, so a target can stand in a red zone you see on the map.

The app remembers the last map, tank and shell (`%APPDATA%\wtv-viewer.json`).

## How it works

```
War Thunder install                         extractor/ (Python + Rust)                  data/ (~1.4 GB)           app
──────────────────────                      ─────────────────────────                   ──────────────            ───
aces.vromfs.bin  tank units, weapons ──┐
lang.vromfs.bin  names               ──┼─ extract.py ──────────────────────────────▶ index.js (maps, tanks) ─┐
mis.vromfs.bin   missions, zones     ──┤   (wt-unpack.exe)                            maps/<level>.js         ├─ bundle ─▶ WT-Ballistic-
levels/<level>.bin heightmap, objects ─┼─ wtv-objects.exe (wt-unpack library) ──────▶ maps/<level>.obj.js     │   (appended    Visualizer.exe
levels/<level>.blk bounds, water     ──┘                                              maps/<level>.tex.js     │    to the exe)  or index.html
tanks/**/<model>.gltf (extracted models) ── tank_models.py ─────────────────────────▶ tanks/<model>.js ──────┘
```

1. **Extraction** (only after a game patch, on a PC with the game):
   - **`extractor/extract.py`** unpacks the game archives with `wt-unpack.exe`. It reads:
     - Every tank's guns, their pitch limits and every shell's speed, mass, calibre and drag coefficient.
     - Every tank mission's battle area, red zones, capture zones and spawns, resolving mission templates and per-game-mode variants.
     - Each level's heightmap, map bounds and water level.
   - **`wtv-objects.exe`** (`extractor/objects`, Rust on wt-unpack's library) exports each level's objects, trees and ground colours. It also flattens the solid objects and the tree crowns into two height grids that line up with the heightmap.
   - **`tank_models.py`** simplifies the extracted glTF tank models to at most 12,000 triangles each.
   - Everything is written to `data/` as small JavaScript files, so the web page can load them without a server.
2. **Bundling:** `rust/src/bin/bundle.rs` appends every file of `data/`, compressed, to the end of the app. It also writes an index and a footer (layout in `rust/src/store.rs`). The lite build stores the data in smaller forms first (see [Lite build](#lite-build)). When the app starts, it looks for that footer in its own file and reads the data from there. Without bundled data it uses the folder in `WTV_DATA`, else a `data` folder next to the exe.
3. **The terrain model** (`core.rs`): the heightmap is interpolated the way the game does it (4 triangles per cell around the cell's mean height). Three surfaces are built from it:
   - bare ground;
   - ground plus solid objects, where tanks stand and what shots hit;
   - ground plus objects plus tree crowns, what blocks sight by default.
4. **Ballistics** (`core.rs`): a point-mass simulation in the vertical plane of the shot. It uses gravity, drag `½·ρ·v²·Cx·πd²/4` and ISA air density by altitude, stepped with Heun's method. The **solver** scans the pitch range in 0.25° steps for elevations that pass through B, then narrows each down by bisection. That gives the low and high arcs, each then checked against the surface along the path.
5. **Sweeps** (`core.rs`, `scene.rs`): one table of trajectories for all elevations, re-used along hundreds of rays around the point. Reach checks every elevation along each ray. Hit target B finds, at each distance, the elevations that end at B's height and checks their paths. Visibility tracks the steepest horizon angle along each ray. The rays are spread over all CPU cores, then drawn into a coloured overlay.
6. **Drawing:**
   - The window, sidebar, 2D map and profile use **GPUI** (Zed's UI framework) with the **gpui-component** widgets.
   - The **3D views** are drawn off screen with **wgpu** (Direct3D 12 or Vulkan): the shaders in `view3d.wgsl` draw terrain patches that get coarser with distance, objects, trees, tanks, water, arcs and zones. Each frame is copied back and shown as an image in the window, with labels, arcs and the sight drawn on top.
   - A frame is only redrawn when something in it changes.

## What has been done

- **Game patch update:**
  - All 56 tank maps were re-extracted with no format changes.
  - American Valley was reworked: smaller map bounds and moved red zones.
  - Small terrain changes on several maps, and 8 new tanks.
  - The level `.blk` files were diffed for size, bounds and spawns.
- **Water** on 28 maps, in 2D and 3D, from each level's `water_level`.
- **UI redesign:** cards, switches and a searchable level list without the `avg_` prefix. "Red zones" was renamed to **Map border** and "Prohibited zones" to **Red zones**.
- **Shell arcs in 3D:** thick paths with a see-through wall to the ground, time dots, apex and impact labels, and dashed hidden parts.
- **Find highest point** inside the map border and outside red zones. It was checked against a scan of every heightmap cell.
- **3D tank models:** your tank at A and a T-72A at B. 1,122 of the 1,219 tanks have a model. 62 use a close variant of the same vehicle, and 74 have none.
- **184 missing tanks added** (1,035 → 1,219):
  - Shells without a drag coefficient in the game files were being dropped. They now get an estimate from shells of the same type, and 506 existing tanks gained 1,013 shells.
  - Vehicles with only autocannons or heavy machine guns are included.
- **Native Rust app:** a full port of the page to GPUI and wgpu, with the ballistics core ported line by line and the same tests.
- **One-file distribution:** all data bundled into the exe, with the C runtime linked statically.
- **Lite build** (560 MB instead of 1.15 GB): one Leopard 2A4 model at A, ground colours at 2048 px, heightmaps and objects stored in packed forms with xz.
- **Gunner sight view** with a range ladder for the selected shell.
- **A and B on rocks and objects:** they now stand on the object's top instead of the ground inside it.
- **Driving A's tank** with W A S D: steering, and pitch and roll that follow the surface under the hull.

## Building from source

### Data (needs the game)

```
cd extractor
python extract.py                      # every tank-mission level; --levels avg_berlin for some only
python tank_models.py                  # tank models from tanks/**/<model>.gltf; --force to redo all
```

The extractor needs:
- Python 3 with Pillow and numpy.
- `wt-unpack.exe`: looked up in `wt-unpack*/`, or pass `--wt-unpack`.
- The game folder: `--game`, default `D:\SteamLibrary\steamapps\common\War Thunder`.

Objects, trees and ground colours need `extractor/objects` built once. Without it the maps have terrain only. With it a full run takes about 15 minutes.

```
winget install Rustlang.Rustup BrechtSanders.WinLibs.POSIX.UCRT
rustup default stable-x86_64-pc-windows-gnu
git clone https://github.com/patodois/wt-unpack wt-unpack-src     # in this folder
cd extractor/objects && cargo build --release
```

Tank models come from glTF files extracted from the game into `tanks/<folder>/<model>.gltf` (+ `.bin`). A unit's model is the `model` key of its unit file. When a model is missing, a variant of the same vehicle stands in when one exists (for example `t_34_1941` uses `t_34_1942`), and the converter prints every stand-in.

| Data file | Source |
|---|---|
| `data/index.js` | maps list; tanks from `aces.vromfs.bin` `tankmodels` + `groundmodels_weapons`, names from `lang.vromfs.bin` |
| `data/maps/<level>.js` | heightmap (`levels/<level>.bin` HM2, zlib + base64 uint16), map bounds and water level (`levels/<level>.blk`), missions and their areas (`mis.vromfs.bin`) |
| `data/maps/<level>.obj.js` | objects (mid-detail meshes, one colour per material), trees and bushes, and the object and tree-crown height grids |
| `data/maps/<level>.tex.js` | ground colours (the land classes blended, up to 4096 px), detail texture index and tiles |
| `data/tanks/<model>.js` | simplified tank model: int16 mm positions + uint16 triangles, zlib + base64 |

### App

```
cd rust
cargo run --release                    # first build ~10 min; needs Visual Studio 2022 (MSVC toolchain, pinned)
cargo test --release                   # 23 tests
cargo build --release && cargo run --release --bin bundle            # dist/WT-Ballistic-Visualizer.exe, ~1.15 GB, ~10 s
cargo build --release && cargo run --release --bin bundle -- --lite  # dist/WT-Ballistic-Visualizer-Lite.exe, ~560 MB, ~3 min
```

Re-bundle after every extraction. Close a running copy of an exe before rebuilding it: Windows won't let a running exe be overwritten.

### Lite build

The lite build has every function of the full one. It saves space as follows (sizes for all 56 maps, inside the 1.15 GB full build → inside the 560 MB lite build):

| Data | Full build | Lite build | Saving |
|---|---|---|---|
| Tank models | 993 models, 51 MB | Leopard 2A4 (shown at A for every tank) and T-72A (B), 0.1 MB | each tank's own model |
| Objects (meshes, collision grids, trees) | zlib + base64, then deflate, 452 MB | unpacked, positions as 16-bit values over each model's bounding box, xz, 233 MB | lossy: about 3 mm on a 200 m model, invisible; collision grids unchanged |
| Heightmaps (with map and mission text) | zlib + base64, then deflate, 332 MB | gradient-predictor residuals (each height minus left + up − up-left), xz, 134 MB | none (lossless) |
| Ground colours and detail textures | 4096 px JPEG and PNGs in base64, then deflate, 228 MB | ground colours at 2048 px (2 m per pixel, the heightmap grid on 4 km maps), JPEG quality 85, 43 MB; detail PNGs as binary, 93 MB | half the resolution of the ground colours when zoomed far in; detail textures unchanged |

In the lite build every large data block (heightmap, objects, images) is its own entry. The data file points at it with `"@<entry>"` instead of holding it as base64 text, and all entries are packed with xz at level 9. The app reads both builds and the plain `data/` folder alike (`payload` and `image_of` in `src/data.rs`, `unpredict` and object version 3 in `src/store.rs` and `parse_objects`).

What was not worth doing:
- **Stripping the program:** the app code is 33 MB and its debug symbols are already in a separate file, so stripping it gains nothing.
- **Other compressors:** zstd packs 17% larger than xz, which only takes about 0.3 s per map to unpack. Recompressing the detail PNGs, or storing them as single-channel images, gains under 5%.
- **Smaller triangle indices:** storing them as 16-bit values saves under 1% once packed.

| File | What it does |
|---|---|
| `src/core.rs` | ballistics, solver, terrain, sweeps: a line-by-line port of the page's `<script id="core">`, keep both in step |
| `src/data.rs` | reads the data files |
| `src/store.rs` | bundled data or data folder; writes bundles; heightmap predictor and object quantisation of the lite build |
| `src/scene.rs` | missions and game-mode variants, zones, aim A → B, highest point, sweeps, hull pose on the surface |
| `src/raster.rs` | relief shading, ground colour composition, water and zone images |
| `src/draw2d.rs` | 2D map and side profile |
| `src/view3d.rs`, `src/view3d.wgsl` | 3D views, overlay, gunner sight |
| `src/app.rs` | window, sidebar, input, driving |
| `src/bin/bundle.rs` | makes the one-file builds (full, and `--lite`) |
| `tools/` | screenshot and input scripts for checks (below) |

For screenshots, `WTV_DEMO=2d|orbit|orbitA|orbitB|aim|sight` places A and B across a capture zone and finds the highest point once a map has loaded, and `WTV_SWEEP=reach|hit|view` also runs a sweep. `bash tools/run.sh avg_normandy us_m3_stuart sight nm` starts the app that way and saves `shots/nm.png` without bringing the window to the front. `tools/input.ps1`, `tools/keys.ps1` and `tools/hold.ps1` (hold a key, for driving) send mouse and key messages to the window. A window covered by another one is only redrawn when it gets input, so send a mouse move (`input.ps1 move x y`) before capturing.

## Web page

`index.html` loads `data/` from its own folder. To share it, copy it together with the `data` folder. It has the same maps, zones, aim, sweeps, highest point, 3D orbit and aim views (WebGL2) and tank models. It does **not** have the gunner sight, and it still puts A and B on the bare ground under objects.

## Model and known limits

- **Ballistics:** no wind, no dispersion, no gun jump. **This has not been checked against in-game shots yet.**
- **Drag:** about 1,400 shells (many WW2 guns) have no `Cx` in their weapon file, and the game's built-in default is not in the data. They use the median `Cx` of shells of the same type and are marked *est. drag*, so their long-range drop is an estimate.
- **Pitch limits** come from the unit's `limits.pitch`. The yaw-dependent limits and hull tilt are ignored.
- **Gunner sight:**
  - The range ladder is worked out for a level shot (target at muzzle height). On slopes of a few degrees, as tank shots usually are, the difference is small. The tick for B is exact.
  - The reticle is a generic one, not each vehicle's own sight.
  - Mils are 1/6400 of a circle, as in the rest of the tool.
- **Objects:**
  - Objects that block shots and sight are those whose collision shapes stop bullets and are at least 2.5 m tall and 6 m² in plan: buildings, big rocks and walls.
  - Each is a solid block up to its highest point, on the heightmap grid. Windows, doorways, gaps under bridges and overhangs let nothing through, and edges are accurate to about one grid cell (2 m on most maps).
  - So A or B clicked right next to a wall can end up on the wall's top.
  - Smaller objects (fences, kerbs, sandbags, debris) are drawn but never block.
- **Trees and bushes:** a crown is a solid disc of the model's radius up to its height. In 3D, trees are stand-in shapes in their leaves' average colour. Trees the game scatters by land class at load time are not in the level files and are missing.
- **Tank models** are untextured and one colour.
- **Driving and hull tilt:**
  - The hull's pitch and roll come from the surface 3.25 m ahead and behind and 1.7 m to each side. The footprint is the same for every tank (6.5 × 3.4 m, a modern MBT).
  - Driving has no physics: no acceleration, no collisions with buildings, and no limit on how steep a slope can be climbed. The tank simply stands on whatever surface is under it.
  - The model is one piece, so the gun points along the hull, not at B. The aim results still give the exact firing solution.
  - The gun pitch limits are not adjusted for the hull's tilt.
- **Water** uses a fixed colour, shown wherever the bare ground lies below the level's water level.
- **Ground:** ground colours are blended colour maps. Up close (3D) the game's detail textures add their pattern but not their colour.
- **Spawn heights:** `node tests/check_alignment.mjs` compares the mission spawn heights with the terrain. On 44 of 56 maps they agree within 3 m. On 12 maps (for example Israel, Red Desert and Northern Valley) the spawns sit 4–19 m higher, probably on static scene geometry that is not extracted.
- **Zone shapes:** a Box spans ±0.5 of its axes (checked). For Sphere and Cylinder areas the radius is assumed to be the axis length (not checked).
- **Map orientation:** +X right, +Z up. This has not been compared with the in-game tactical map.
- **Weapons:** ATGMs and rockets are skipped. Vehicles without a cannon list their autocannons or heavy machine guns.

## Tests

```
cd rust && cargo test --release                               # app: core, data, zones, highest point, sweeps, standing on objects, bundling, lite formats
node --test tests/core.test.mjs                               # web page core
node --max-old-space-size=8000 tests/check_alignment.mjs      # data check after extraction
```
