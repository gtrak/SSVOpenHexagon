# lib-sfml — SFML (vittorioromeo fork)

Pin: **`vittorioromeo/SFML@dacceddbd1adee49c7bfd120266e200800550394`**
(`CMakeLists.txt:108-112`, CPM, `SFML_STATIC_LIBRARIES` on, PCH on, C++20).
Every `file:line` citation below is a file in **that pinned tree**. Local
checkout: `/tmp/sfml_fork`. Note the checkout's `HEAD` is a *different* side
branch tip (`4963c3f`, `git describe` = `3.0.1-1664-g4963c3f6`) with a dirty
index; every file cited here was verified **byte-identical to the pin** via
`md5sum(worktree) == md5sum(git show dacceddb…:<path>)` before citing.

Scope: **the drawing-layer surface only** (issue 011 / D-011). The used-symbol
set was derived with `grep -rho 'sf::[A-Za-z_:0-9]*' src/ include/ | sort -u`
on this repo. Cross-references:

- `.agents/plans/001-rust-port/mapping/oracle-vectors.md` — "SFML `Packet`
  encoding rules", "SFML key/button enum values", and the byte-exact replay
  vectors. That file is the ground truth for those; cited, not duplicated.
- `.agents/skills/lib-render-stack.md` — the Rust (`winit`+`glutin`+`glow`…)
  analogue that ports this surface.
- `.agents/skills/lib-binds.md` — the SSVStart `KKey`/`MBtn` bind layer on top
  of the SFML enums.

## `sf::Vector2<T>` (`include/SFML/System/Vector2.hpp:42`)

- `constexpr Vector2()` zero (`:51`); public members `x`, `y`.
- Free operators live in `Vector2.inl`: unary `-` `:105`, `+=` `:113`, `-=`
  `:124`, `+` `:135`, `-` `:143`, `*scalar` `:151`/`:159`, `*=` `:167`, `/`
  `:178`, `/=` `:186`, `==` `:197`, `!=` `:205`. **There is no
  Vector2×Vector2 product** (dot/cross come from the game's own helpers,
  `extlibs/SSVUtils`).
- Game usage: `sf::Vector2f` is the gameplay coordinate (walls/player:
  `src/SSVOpenHexagon/Components/CWall.cpp:26`, `CPlayer.cpp:96-200`; geometry
  `include/SSVOpenHexagon/Utils/Geometry.hpp:14-22`); `sf::Vector2i` for mouse
  deltas (`src/SSVOpenHexagon/Core/MenuGame.cpp:284`); `sf::Vector2u` for
  sizes (`MenuGame.cpp:1047`, `Global/Config.cpp:643`).
- **`sf::Vector3*` is not used anywhere in the repo** (grep-verified) — the
  Rust port may ignore it.

## `sf::Color` (`include/SFML/Graphics/Color.hpp:41`)

- Public members `r,g,b` (`std::uint8_t`, default 0) and `a{255}`
  (`Color.hpp:98-101`). `constexpr Color()` = opaque black;
  `Color(r,g,b,a=255)`; `explicit Color(std::uint32_t)`; `toInteger()`.
- Predefined colors are `inline constexpr` in `Color.inl:146-154`:
  `Black(0,0,0)`, `White(255,255,255)`, `Red(255,0,0)`, …,
  `Transparent(0,0,0,0)`.
- Arithmetic in `Color.inl`: `*` is modulation (component product **divided
  by 255**) `:104`; `+=` `:120` clamps at 255; `-=` `:127` clamps at 0; `*=`
  `:134`.
- Game usage: `window->clear(sf::Color::Black)` (`Core/HGGraphics.cpp:83`);
  statics `Black/White/Red/Transparent` used across menu/game code.

## `sf::Vertex` (`include/SFML/Graphics/Vertex.hpp:40`)

- Ctors: `()` `:47`; `(pos)` `:57`; `(pos, color)` `:68`; `(pos, texCoords)`
  `:79`; `(pos, color, texCoords)` `:89`.
- Members: `Vector2f position` `:94`, `Color color{Color::White}` `:95`,
  `Vector2f texCoords` `:96` (default (0,0)).
- The type is a **plain POD layout (position, color, uv)** and the game relies
  on that: `FastVertexVector` (below) `static_assert`s
  `sizeof(sf::Vertex)`/`alignof` against its own union and `std::memcpy`s
  blocks of vertices (`include/SSVOpenHexagon/Utils/FastVertexVector.hpp:37-38,
  61,86`).

## `sf::PrimitiveType` (`include/SFML/Graphics/PrimitiveType.hpp:38-45`)

```cpp
enum class PrimitiveType { Points, Lines, LineStrip,
                           Triangles, TriangleStrip, TriangleFan };
```

The game uses **only `Triangles`** (`FastVertexVectorTris` =
`FastVertexVector<sf::PrimitiveType::Triangles>`,
`FastVertexVector.hpp:177`; quad emission = two triangles,
`FastVertexVector.hpp:181-202`).

## `sf::Drawable` / `sf::RenderTarget` / `sf::RenderStates`

- `Drawable` (`include/SFML/Graphics/Drawable.hpp:43`): protected pure virtual
  **`virtual void draw(RenderTarget& target, const RenderStates& states) const = 0`**
  (`:66`); `RenderTarget` is the only friend. This exact signature is the
  port contract.
- Game subclass: `hg::Utils::FastVertexVector<TPrimitive>`
  (`FastVertexVector.hpp:24`) emits via
  `RenderTarget::draw(const Vertex*, std::size_t, PrimitiveType, const
  RenderStates&)` (`RenderTarget.hpp:260`) with a `reinterpret_cast` from its
  raw array (`FastVertexVector.hpp:132-133`, marked "UB:" by the author — it
  is safe only because of the POD layout above).
- `RenderTarget` (`include/SFML/Graphics/RenderTarget.hpp:53`): `clear(color =
  Color(0,0,0,255))` `:83`; `setView` `:104` / `getView` `:114` /
  `getDefaultView` `:127`; `getViewport(view)` `:142`;
  `mapPixelToCoords` `:162` (current view) / `:193` (explicit view);
  `mapCoordsToPixel` `:213` / `:240`; `draw(Drawable, states =
  RenderStates::Default)` `:249`; `draw(Vertex*, count, type, states)` `:260`;
  `draw(VertexBuffer…)` `:272`/`:283`.
- `RenderStates` (`include/SFML/Graphics/RenderStates.hpp`): ctors
  `()`/`(BlendMode)`/`(Transform)`/`(Texture*)`/`(Shader*)`/full
  (`:60-103`); **`static const RenderStates Default`** `:109`, defined at
  `src/SFML/Graphics/RenderStates.cpp:38` (BlendAlpha + identity transform +
  null texture + null shader); members `:113-117`:
  `blendMode{BlendAlpha}`, `transform`, `const Texture* texture{}` (null),
  `const Shader* shader{}` (null).
- Game usage: `sf::RenderStates{assets.getShaderByShaderId(*id)}`
  (`Core/HGGraphics.cpp:77`), `RenderStates::Default` as the fallback
  (`HGGraphics.cpp:62-79`).

## `sf::Texture` / `sf::Sprite`

- `Texture` (`include/SFML/Graphics/Texture.hpp:51`): `create(Vector2u)`
  `:114`; `loadFromFile(path, area=IntRect())` `:145`; `loadFromMemory`
  `:177`; `loadFromImage` `:232`; `getSize()` `:240`; `setSmooth` `:417` /
  `isSmooth` `:427`; `setRepeated` `:486`.
- Game usage: single-load cache `value.loadFromFile(path)` in
  `Global/AssetStorage.cpp:35-38`; `setSmooth(true)`
  (`Core/HexagonGame.cpp:97`, `Core/MenuGame.cpp:141,142,826`).
- `Sprite` (`include/SFML/Graphics/Sprite.hpp:47`, inherits
  `Drawable + Transformable`): `setTexture(const Texture&, resetRect=false)`
  `:110`; `setTextureRect` `:130`; `setColor` `:145`; `getLocalBounds` `:193`;
  `getGlobalBounds` `:207`. Game usage: key icons + replay icon
  (`include/SSVOpenHexagon/Core/HexagonGame.hpp:174-214`), particle sprites
  with rotation (`src/SSVOpenHexagon/Core/HGUpdate.cpp:929-966`).

## `sf::Font` / `sf::Text`

- `Font` (`include/SFML/Graphics/Font.hpp:57`): `loadFromFile` `:125`;
  `loadFromMemory` `:146`; `getGlyph(codePoint, characterSize, bold,
  outlineThickness=0)` `:200`; `getLineSpacing(characterSize)` `:249`;
  `setSmooth` `:308`.
- Game usage: `AssetStorage::loadFont` (`Global/AssetStorage.cpp:62-64,
  129-133`); null-font fallback (`Global/Assets.cpp:588`).
- `Text` (`include/SFML/Graphics/Text.hpp:49`, inherits
  `Drawable + Transformable`): `setString` `:139`; `setFont(const Font&)`
  `:156` (rvalue overload **deleted** `:162`); `setCharacterSize` `:181`;
  `setLineSpacing` `:195`; `setLetterSpacing` `:214`; `setStyle` `:228`;
  `setFillColor` `:242`; `setOutlineColor` `:254`; `setOutlineThickness`
  `:269`; `getLineSpacing` `:332`; `getLocalBounds` `:403`;
  `getGlobalBounds` `:417`.
- Game usage: the height probe
  `font.setString("ABCDEFGHILMNOPQRSTUVZ:"); return
  font.getGlobalBounds().height;` (`src/SSVOpenHexagon/Utils/FontHeight.cpp:13-14`).

## Shapes: `sf::Shape` / `sf::Transformable` / `sf::RectangleShape`

- `Shape` (`include/SFML/Graphics/Shape.hpp:46`, inherits
  `Drawable + Transformable`): `setTexture` `:69`; `setFillColor` `:100`;
  `setOutlineColor` `:112`; `setOutlineThickness` `:127`;
  `getLocalBounds` `:222`; `getGlobalBounds` `:243`.
- `Transformable` (`include/SFML/Graphics/Transformable.hpp:42`):
  `setPosition` `:69`; `setRotation(Angle)` `:83`; `setScale` `:97`;
  `setOrigin` `:114` (+ matching getters; no `setSkew` in this fork —
  grep-verified).
- `RectangleShape` (`include/SFML/Graphics/RectangleShape.hpp:41`):
  `setSize(Vector2f)` `:60`.
- Game usage: `levelInfoRectangle` (`Core/HexagonGame.cpp:174-190`:
  setSize/setScale/setFillColor/setOutlineColor/setOrigin/setOutlineThickness/
  setPosition) and `rsOnlineStatus` (`Core/MenuGame.cpp:6089-6094`).

## `sf::View` / `sf::Transform`

- `View` (`include/SFML/Graphics/View.hpp:44`): `View()` `:53`;
  `explicit View(const FloatRect&)` `:61`; `View(center, size)` `:70`;
  `setCenter` `:80`; `setSize` `:90`; `setRotation` `:102`; `setViewport`
  `:119`; `reset` `:131`; `getViewport` `:171`; `zoom(factor)` `:208`;
  `getTransform` `:220`; `getInverseTransform` `:232`.
- `Transform` (`include/SFML/Graphics/Transform.hpp:44`): `getMatrix()` `:86`
  (16 `float`s, column-major GL sense — consumed by `glLoadMatrixf` per its
  own docs at `:80`); `operator*(Transform, Transform)` `:288`;
  `operator*(Transform, Vector2f)` `:316`.
- `FloatRect = Rect<float>` (`include/SFML/Graphics/Rect.hpp:172`).
- Game usage: game cameras are `ssvs::Camera` wrappers that hold
  `sf::View view, computedView` and expose `apply() -> sf::View`
  (`extlibs/SSVStart/include/SSVStart/Camera/Camera.hpp:28,56`); the window
  gets `window->setView(backgroundCamera->apply())`
  (`Core/HGGraphics.cpp:109,122,280`); direct `sf::View{sf::FloatRect{…}}`
  construction (`Core/MenuGame.cpp:3142,6002`).

## `sf::RenderTexture` (`include/SFML/Graphics/RenderTexture.hpp:50`)

Inherits `RenderTarget`; `create(size, settings)` `:87`; `display()` `:185`;
`getTexture() const -> const Texture&` `:224`.
**Not used by the game** — only the includes exist
(`include/SSVOpenHexagon/Core/HexagonGame.hpp:36`,
`src/SSVOpenHexagon/Core/HGGraphics.cpp:27`). The Rust port implements the
offscreen-FBO case directly with `glow` instead
(`crates/oh-render/tests/golden_frames.rs`, see `lib-render-stack.md`).

## `sf::Shader` / `sf::Glsl`

- `Shader` (`include/SFML/Graphics/Shader.hpp:53`, inherits `GlResource`).
  `enum Type` is a **plain enum** `:60-65`: `Vertex, Geometry, Fragment`.
  `loadFromFile(path, Type)` `:145`; `loadFromFile(vs, fs)` `:166`;
  `loadFromMemory(shader, Type)` `:211`.
- Uniform setters exist in two flavours: `setUniform(...)` (safe,
  `:330-…`) and **`setUniformUnsafe(...)` `:594-674`** — `float` `:594`,
  `Glsl::Vec2` `:603`, `Vec3` `:612`, `Vec4` `:630`, `int` `:639`,
  `Ivec2` `:648`, `Ivec3` `:657`, `Ivec4` `:674`. The game calls **only the
  `setUniformUnsafe` variants** through the `shdr_*` Lua bindings
  (`src/SSVOpenHexagon/Core/LuaScripting.cpp:1382,1397,1414,1432,1453,1468`).
  (No claim about upstream SFML 3.0.1 is made here — no upstream source was
  consulted.)
- Game usage: shaders loaded from pack files with
  `shader->loadFromFile(p.getStr(), shaderType)`
  (`src/SSVOpenHexagon/Global/Assets.cpp:889,1163`); one fragment shader per
  render stage bound via `RenderStates` (`Core/HGGraphics.cpp:55-79`).
- `sf::Glsl` (`include/SFML/Graphics/Glsl.hpp`): `Vec2 =
  sf::Vector2<float>` `:65`; `Ivec2 = sf::Vector2<int>` `:71`; `Vec3 =
  sf::Vector3<float>` `:83`; `Ivec3`/`Vec4`/`Ivec4` analogous; `Vec4` struct
  in `Glsl.inl:92` with an implicit `Vec4(Color)` ctor `Glsl.inl:142`
  (channels **normalized to [0,1]**).

## `sf::Clock` / `sf::Time` / `sf::degrees`

- `Clock` (`include/SFML/System/Clock.hpp:91`): SFML-3.x shape — **there is no
  `elapsed()` member**; the whole public API is
  `Time restart()` `:115` (resets the clock and returns time elapsed since the
  last restart/construct). Elapsed time is measured by
  `SuspendAwareClock` (`Clock.hpp:72`, `include/SFML/System/
  SuspendAwareClock.hpp`), which excludes process-suspend intervals.
- Game usage: `ilcDeltaClock.restart()` feeds the imgui frame delta
  (`src/SSVOpenHexagon/Core/HGUpdate.cpp:1314`; field
  `include/SSVOpenHexagon/Core/HexagonGame.hpp:108`).
- `Time` (`include/SFML/System/Time.hpp:42`): `Zero` `:111`; factories
  `seconds(float)` `:135`, `milliseconds(std::int32_t)` `:148`,
  `microseconds(std::int64_t)` `:161`; `asSeconds()` `:68`,
  `asMilliseconds()` `:78`; arithmetic/relational operators `:244-…`.
- `sf::degrees` (`include/SFML/System/Angle.hpp`: class `Angle` `:41`,
  `degrees(float)` `:175`, `radians(float)` `:187`). Game usage:
  `sprite.setRotation(sf::degrees(…))`
  (`src/SSVOpenHexagon/Core/HGUpdate.cpp:945,966`, `Core/HexagonGame.cpp:124`).
- `Time` is also the unit for socket timeouts / audio offsets outside the
  draw layer (`Core/HexagonClient.cpp:103`, `Core/HexagonServer.cpp:301`,
  `Core/Audio.cpp:109,117`).

## `sf::Packet` (`include/SFML/Network/Packet.hpp:48`)

Surface used by the game: `append(data, n)` (`:99`), `getData()` `:136`,
`getDataSize()` `:149`, `endOfPacket()` `:163`, `operator bool()` `:203`,
`operator<<`/`operator>>` for `bool, i8/u8, i16/u16, i32/u32, i64/u64,
float, double, const char*, std::string, wchar_t*, std::wstring, sf::String`
(`:209-365`).

**Encoding rules** (verified in `src/SFML/Network/Packet.cpp`; the
byte-exact replay consequences are in `oracle-vectors.md`):

| type | write (`Packet.cpp`) | read | wire format |
|---|---|---|---|
| `bool` | `:393` | `:119` | 1 byte (0/1) |
| `i8`/`u8` | `:401`/`:409` | `:130`/`:143` | 1 byte raw |
| `i16`/`u16` | `htons` `:419`/`:428` | `ntohs` `:161`/`:175` | 2 bytes **big-endian** |
| `i32`/`u32` | `htonl` `:437`/`:446` | `ntohl` `:189`/`:203` | 4 bytes **big-endian** |
| `i64`/`u64` | manual byte shifts `:455`/`:473` | `:216`/`:238` | 8 bytes **big-endian** |
| `float`/`double` | raw `append` `:493`/`:501` | raw `memcpy` `:256`/`:269` | **host-endian** (LE on x86), NOT swapped |
| `const char*`/`std::string` | u32 length + bytes `:509`/`:523` | `:282`/`:303` | u32 **BE** length, then raw bytes |
| `wchar_t*`/`wstring`/`String` | `:538`/`:553`/`:571` | `:324`/`:347`/`:370` | u32 **BE** char count, each char as u32 |

- Overflow behaviour: reads go through `checkSize` (`Packet.cpp:589`); a
  short read sets the packet invalid so `operator bool`
  (`Packet.hpp:203`, `Packet.cpp:112`) becomes `false`.
- Game usage: `replay_file::serialize_to_packet` /
  `deserialize_from_packet` (`src/SSVOpenHexagon/Core/Replay.cpp:380,397,467,475`)
  and the online protocol encoders (`src/SSVOpenHexagon/Online/Shared.cpp:92-120`).
- **The big-endian-vs-LE trap** (replay *file* = host LE via `std::memcpy`,
  replay *packet* = `sf::Packet` BE) and the exact expected hex bytes are in
  `oracle-vectors.md` ("Replay serialization" + "SFML `Packet` encoding
  rules") — read it before touching any of this.

## `sf::Keyboard::Key` / `sf::Mouse::Button`

- `enum Key` (`include/SFML/Window/Keyboard.hpp:51-156`): `Unknown = -1`
  `:53`, sequential letters/digits/navigation, `KeyCount` `:156` (= 101).
- `enum Button` (`include/SFML/Window/Mouse.hpp:49-57`): `Left = 0` …
  `XButton2 = 4`, `ButtonCount` `:57` (= 5).
- **Full integer tables**: `oracle-vectors.md` "SFML key/button enum values"
  (do not re-derive or retyping; those values are the bind-serialization
  surface, see `lib-binds.md`).

## sf:: symbols NOT covered by this skill (outside the draw layer)

Grep-verified usages that belong to other porting areas: `sf::Event` /
`sf::RenderWindow` / `sf::VideoMode` (window+input; Rust analogue:
`lib-render-stack.md`), `sf::Joystick` (`Core/Joystick.cpp`; stubbed in Rust),
`sf::Image` (`Core/main.cpp:315` — window icon only), `sf::Music` /
`sf::SoundBuffer` (audio; Rust: `rodio` in `lib-render-stack.md`),
`sf::IpAddress` / `sf::Socket*` / `sf::TcpListener` / `sf::TcpSocket` /
`sf::UdpSocket` / `sf::SocketSelector` (network; ported by issues 16-18).

## Claims NOT verified (stated honestly)

- Nothing here was compared against upstream SFML (no network); "fork
  specific" is only asserted where the fork's own source shows it
  (`setUniformUnsafe`, `SuspendAwareClock`, header-only `Color`).
- Line numbers refer to the pinned tree; if `/tmp/sfml_fork` is re-checked-out,
  re-verify with the md5-against-pin method above before trusting them.
