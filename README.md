# WT Ballistic Visualizer

An offline map and ballistics tool for War Thunder ground battles. Place your tank and a target on any tank map, and it shows whether and how your shell gets there: the gun elevation, the hold over the target, and the shell's path in 2D, in 3D and through a gunner's sight. It also shows each mission's map border and red zones.

## Download

Get `WT-Ballistic-Visualizer-Lite.exe` from [Releases](../../releases/latest) and run it (64-bit Windows 10 or 11). It's one file with all map and tank data inside, so you don't need the game or anything else installed.

Windows may say "Windows protected your PC" because the file is not signed: click **More info**, then **Run anyway**.

## Using it

1. Pick a **Level**, **Tank** and **Shell** in the sidebar. The level and tank lists can be searched by typing.
2. **Left click** the map to place your tank at **A**, and **right click** to place the target at **B**. To take them off the map again, use **Remove A**, **Remove B** or **Remove both** in the **Aim A → B** card, or press **Delete** to remove both.
3. The **Aim A → B** card shows:
   - the distance, and whether the sight line is clear;
   - for the low and high arc: whether it hits, the gun elevation, how far to aim over B, the flight time and the impact speed.
4. Change **View** to see the shot:
   - **2D map:** the map from above.
   - **3D orbit:** drag to rotate, right drag to pan, wheel to zoom, double click to centre.
   - **3D aim:** looks from A's gun at B.
   - **3D gunner sight:** the same view through a sight, with a mil scale and range marks for your shell. Mouse wheel = magnification.

| Key | Action |
|---|---|
| W / S (or ↑ / ↓) | drive A's tank forward / back |
| A / D (or ← / →) | steer A's tank |
| R | map border on / off |
| / | search levels |
| Delete | remove A and B |

Keys work after you click the map. The tank follows the slope of the ground, and A and B stand on top of rocks and objects.

**More tools:**
- **Find highest point:** the highest ground inside the battle area that isn't in a red zone.
- **Reach from A:** everywhere a target can be hit from A.
- **Hit target B:** everywhere you could shoot B from. Green means the low arc works, orange means only the high arc does, blue means both.
- **Visibility from A:** what A can see, and what blocks the view.
- **Layers:** switch the map border, red zones, capture zones, water, grid, objects, trees and tank models on or off. You can also choose whether trees block sight and shots.
- **Choose red zones** (under the Red zones switch) lists every red zone of the selected mission, with the team it applies to. Each zone has a number, shown in a badge on the map (2D and 3D) and in front of its entry in the list, so you can match them. Zones that are off keep a grey badge. Turn single zones off, or all of them with **All off**. A zone that's off is hidden and no longer counts for **Find highest point** and **Hit target B**. The list resets when you change map.
- **Game mode and mission:** zones differ per mode (Arcade, Realistic, Simulator) and per mission.

## Good to know

- **Not checked against the game:** the ballistics have not been compared with in-game shots. There's no wind, no dispersion and no gun jump.
- **Estimated drag:** shells marked *est. drag* have no drag value in the game files, so they use an estimate, and their drop at long range is approximate.
- **Objects:** buildings, big rocks and walls block shots as solid blocks up to their top, so windows and gaps don't let anything through. Smaller objects (fences, debris) never block.
- **Trees** are solid crowns that block sight by default but not shots.
- **Tank models:** a Leopard 2A4 stands at A for every tank and a T-72A at B. The models are untextured.
- **Driving** has no physics: the tank goes over walls and up any slope, and its gun points along the hull, not at B.
- **Range marks:** the gunner sight's marks are worked out for level ground. The aim mark for B is always exact.
- **After a game patch** maps can change, so use the newest release.

## Building from source

Updating the data needs the game installed, Python 3 with Pillow and numpy, and [wt-unpack](https://github.com/patodois/wt-unpack) (`wt-unpack.exe` in a `wt-unpack*/` folder here). The app needs Rust and Visual Studio 2022.

```
cd extractor
python extract.py           # maps, missions, tanks and shells into ../data  (--game "<War Thunder folder>")
python tank_models.py       # tank models from ../tanks/**/<model>.gltf

cd ../rust
cargo run --release                                                  # run the app on ../data
cargo test --release                                                 # tests
cargo build --release && cargo run --release --bin bundle           # dist/WT-Ballistic-Visualizer-Lite.exe
```

Objects, trees and ground colours need `extractor/objects` built once (`cargo build --release` there, with wt-unpack's source in `wt-unpack-src/`).
