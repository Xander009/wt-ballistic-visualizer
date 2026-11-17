# WT Ballistic Visualizer

An offline map and ballistics tool for War Thunder ground battles. Place your tank and a target on any tank map, and it shows whether and how your shell gets there: the gun elevation, the hold over the target, and the shell's path in 2D, in 3D and through a gunner's sight. It also shows each mission's map border and red zones.

## Download

Get one of the two from [Releases](../../releases/latest) and run it (64-bit Windows 10 or 11). Each is one file with all map and tank data inside, so you don't need the game or anything else installed.

- **`WT-Ballistic-Visualizer-Lite.exe`:** the tank maps.
- **`WT-Ballistic-Visualizer-Max.exe`:** the same, plus the whole level around each tank map (about 65 km across) with its airfields and heli pads. It's a bigger download.

Windows may say "Windows protected your PC" because the file is not signed: click **More info**, then **Run anyway**.

## Using it

1. Pick a **Level**, **Tank** and **Shell** in the sidebar. The level and tank lists can be searched by typing.
2. **Left click** the map to place your tank at **A**, and **right click** to place the target at **B**. In the Max build both can go anywhere on the whole level, for example on a heli pad or an airfield.
3. The **Aim A → B** card shows:
   - the distance, and whether the sight line is clear;
   - for the low and high arc: whether it hits, the gun elevation, how far to aim over B, the flight time, the impact speed and the gun's spread at B.
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

Keys work after you click the map. The tank follows the slope of the ground, and A and B stand on top of rocks and objects.

**Compass:** the 3D views have the game's compass at the top. It reads where the view looks: from A at B in the aim and sight views, the camera's heading in the orbit view. The orange marker on it is A's hull, and the blue line with **B** and its bearing is the direction of B from A (an arrow at the end of the strip when B is outside it). Tank spawns have an arrow on the map for the way a tank spawned there faces, with its compass reading when zoomed in. A placed in a spawn faces that way, so the compass shows what you'd see when you first spawn there.

**More tools:**
- **Find highest point:** the highest ground inside the battle area that isn't in a red zone.
- **Reach from A:** everywhere a target can be hit from A.
- **Hit target B:** everywhere you could shoot B from. Green means the low arc works, orange means only the high arc does, blue means both.
- **Visibility from A:** what A can see, and what blocks the view.
- **Layers:** switch the map border, red zones, capture zones, aircraft and helicopter spawns (Max), water, grid, objects, trees and tank models on or off. You can also choose whether trees block sight and shots.
- **Choose red zones** (under the Red zones switch) lists every red zone of the selected mission, with the team it applies to. Each zone has a number, shown in a badge on the map (2D and 3D) and in front of its entry in the list, so you can match them. Zones that are off keep a grey badge. Turn single zones off, or all of them with **All off**. A zone that's off is hidden and no longer counts for **Find highest point** and **Hit target B**. The list resets when you change map.
- **Game mode and mission:** zones differ per mode (Arcade, Realistic, Simulator) and per mission.
- **Battle rating (Max):** some missions load different airfields, heli pads and AA guns depending on the battle's rank (for example a small airfield with 37 mm and 85 mm guns up to BR 7.7, a 3 km airfield from BR 8.0). Pick the rating under the mission; the choice only appears when the mission has such parts.
- **Shell spread:** the outline (filled) on the ground in 2D, and a ring in 3D, shows where a shell aimed at B can land, from the gun's dispersion in the game files. Without the gun's fire-control modules (Adjustment of Fire, Elevation Mechanism; switch *Fire-control modules researched* off) the spread is wider, by the factors the game files give.
- **Whole level (Max only):** zoom out in the 2D map or the 3D orbit view to see the land, sea and rivers around the tank map, with the airfields, heli pads and other objects the selected mission places (runways, hangars, tents, parked vehicles). Airfields and heli pads have the game's own ground textures (runways, taxiways, H marks). Place A or B out there to see whether a shell reaches a heli pad or an airfield; the sweeps follow the ground there too.
- **Aircraft and helicopter spawns (Max only):** the spots helicopters spawn on at each heli pad and aircraft at each airfield (small dots, a label per pad or airfield), the anti-aircraft guns guarding them (red squares, with the gun's name when zoomed in; at airfields the guns sit in the airfield's AA slots, and hollow squares are slots the mission leaves empty), and the spawns in the air (dashed circles with the team, the mode and the height). In 3D the spots and guns are short poles and the air spawns rings at their height.

## Good to know

- **Not checked against the game:** the ballistics have not been compared with in-game shots. There's no wind and no gun jump.
- **Spread:** the game files hold each gun's largest deviation from the bore line and the modules' factors. Crew skills, barrel heat and anything the server adds are not in them, and how shots are spread inside the outline isn't either, so the outline is the edge of where shells can go, not a hit chance.
- **Estimated drag:** shells marked *est. drag* have no drag value in the game files, so they use an estimate, and their drop at long range is approximate.
- **Objects:** buildings, big rocks and walls block shots as solid blocks up to their top, so windows and gaps don't let anything through. Smaller objects (fences, debris) never block.
- **Trees** are solid crowns that block sight by default but not shots.
- **Tank models:** a Leopard 2A4 stands at A for every tank and a T-72A at B. The models are untextured.
- **Spawn heading:** read from each spawn zone's rotation in the mission file (the zone's x axis, which points at the enemy's spawns in 96 % of zones). It hasn't been checked in game, and a mission can turn spawned tanks towards a unit instead.
- **Driving** has no physics: the tank goes over walls and up any slope, and its gun points along the hull, not at B.
- **Range marks:** the gunner sight's marks are worked out for level ground. The aim mark for B is always exact.
- **Whole level (Max):** the ground outside the tank map has one height every 32 m, so it's coarse up close. Around airfields and heli pads it has one every 8 m, the finest the game's terrain mesh has there; the game holds no finer heights for them. Airfields stand on level ground: a flat platform at the ground height under the airfield's centre, with an embankment down to the ground around it (the game's airfield mesh has that shape; where the platform height comes from is a best reading of the files). Heli pad textures lie on the ground and don't change its shape. The game's grime decals on them (stains, tyre marks) are left out. Airfield and heli pad buildings (hangars, towers, barracks, caponiers) block shots and sight and A or B can stand on them, as solid blocks from their drawn shape. They block for every mission's airfields, even ones the selected mission doesn't place. Otherwise outside the tank map only the ground blocks shots and sight: its buildings and trees are only drawn. Other mission objects (tents, props) are only drawn, even on the tank map. Sweep results show in 3D only on the tank map; the 2D map shows them everywhere.
- **After a game patch** maps can change, so use the newest release.

## Building from source

Updating the data needs the game installed, Python 3 with Pillow and numpy, and [wt-unpack](https://github.com/patodois/wt-unpack) (`wt-unpack.exe` in a `wt-unpack*/` folder here). The app needs Rust and Visual Studio 2022.

```
cd extractor
python extract.py           # maps, missions, tanks and shells into ../data  (--game "<War Thunder folder>")
                            # add --far for the Max build: whole levels, airfields and heli pads
python tank_models.py       # tank models from ../tanks/**/<model>.gltf

cd ../rust
cargo run --release                                                  # run the app on ../data
cargo test --release                                                 # tests
cargo build --release && cargo run --release --bin bundle           # dist/WT-Ballistic-Visualizer-Lite.exe
cargo run --release --bin bundle -- --max                            # dist/WT-Ballistic-Visualizer-Max.exe (needs --far data)
```

Objects, trees and ground colours need `extractor/objects` built once (`cargo build --release` there, with wt-unpack's source in `wt-unpack-src/`).
