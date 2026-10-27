# Reading a tellers OTIO timeline

A tellers timeline is an [OpenTimelineIO](https://opentimelineio.readthedocs.io)
(OTIO) JSON file. Any OTIO tool can open it. Tellers adds data in two places:
`metadata["tellers.ai"]` on any object, and **tellers effects**. This guide
shows how to read one file: what plays where, when, and how it looks. For the
full list of objects, fields and effects, see the generated
[reference](otio-reference.md).

## The tree

```
Timeline.1
└── tracks: Stack.1                 every track, composited together
    ├── effects: [...]              LUTs over the whole picture
    └── children:
        ├── Track.1  kind: video    bottom layer
        │   ├── effects: [...]      applied to each clip of the track
        │   └── children:
        │       ├── Gap.1           empty time
        │       ├── Clip.2          media or generated content
        │       │   ├── media_references: { DEFAULT_MEDIA: ExternalReference.1 | GeneratorReference.1 }
        │       │   └── effects: [...]   applied to this clip only
        │       └── Clip.2 ...
        ├── Track.1  kind: video    drawn above the previous track
        └── Track.1  kind: audio    sound
```

Every object says what it is in `OTIO_SCHEMA`, e.g. `Clip.2`.

## Time

**A time** is a `RationalTime`: `value / rate` seconds. For example,
`{ "value": 48, "rate": 24 }` is 2 s. Tellers usually writes `rate: 1`, so
`value` is the time in seconds.

**Clips have no start time.** A track plays its items one after the other.
Each item starts where the previous one ends, so its start on the timeline is
the sum of the durations of the items before it. Gaps hold empty time.

**What a clip plays** is set by its `source_range`:

- `duration` is how long the clip lasts, both on the track and in the media.
- `start_time` is where it starts *in the media's own time*. With
  `start_time = 10 s` and `duration = 4 s`, the clip plays seconds 10–14 of
  its file.

**Where a clip sits on the timeline**, worked through for a track with these
items:

| Item | `source_range` (start, duration) | On the timeline |
|---|---|---|
| Gap | (0, 1) | 0 s – 1 s, nothing |
| Clip A | (10, 2.5) | 1 s – 3.5 s, plays A from 10 s to 12.5 s |
| Clip B | (0, 4) | 3.5 s – 7.5 s, plays B from 0 s to 4 s |

**Every track starts at 0.** The timeline is as long as its longest track.

**Times in effects and markers** use different clocks:

- Keyframe `t` in an effect is seconds from the start of the *clip on the
  timeline*, not media time.
- A marker's `marked_range` is in timeline time on the stack and tracks, and
  in media time on a clip.

## Layers and tracks

- **Layer order:** tracks render in list order. The first video track is the
  bottom layer and the last is the top one. Tellers keeps audio tracks at the
  bottom of the list.
- **Track kinds:**
  - **Video** tracks carry picture only.
  - **Audio** tracks carry sound only. A video file's sound plays from a
    separate clip on an audio track.
  - **Other** tracks are kept but not played.
- **Sync partners:** a video clip and its audio clips share a Resolve
  `Link Group ID` in their `metadata.Resolve_OTIO`. They move and trim
  together. Nothing else stores this link: tellers works it out from those
  ids each time.
- **Disabling:** setting `enabled: false` on a track or a clip mutes and
  hides it. A disabled clip still takes up its time on the track.

## Media references: what a clip shows

`active_media_reference_key` picks the reference that plays from
`media_references`. It defaults to `DEFAULT_MEDIA`.

- **Media files** use `ExternalReference.1`. `target_url` is the file.
  - Tellers fills in URLs from asset ids before playback and empties them
    before saving, so a stored timeline often has `"target_url": ""`.
  - `available_range` is the part of the file that exists. The clip's
    `source_range` stays inside it.
  - `MissingReference.1` is drawn as a gap by the player. The library reads
    it as an external reference with no URL and writes it back as
    `ExternalReference.1`.
- **Generated content** uses `GeneratorReference.1`, and `generator_kind` says
  which generator:
  - `Rich`: Resolve **Rich Text**. The HTML is in
    `parameters.Resolve_OTIO[…]` (`Effect Name: "Rich Text"`), under the
    `title blob` parameter's `Title HTML`. Its `position` (0–1, centered at
    0.5), zoom and rotation place the text box. Fonts come from
    `availableFonts`.
  - `Text`: **plain text**.
    - `parameters.text`, or `parameters.words` (`[{text, start, end?}]` with
      times in seconds, for karaoke-style captions)
    - `maxWidth` and `wrap`
    - `style`: `fontFamily`, `fontSize`, `fill`, `align`, `stroke`,
      `dropShadow`, and a `background` box with `mode` `block` / `line` / `word`. Sizes are in pixels of the
      timeline's text render size. An `align`, `fontStyle` or background
      `mode` the player does not know is drawn as its default (`center`,
      `normal`, `block`).
  - `Shape`: a **shape**.
    - `parameters.shape` is `rectangle`, `ellipse` or `line`
    - `fill`, `stroke`, `stroke_width` and `corner_radius`, in pixels of a
      1080-high frame
    - line ends `x1`/`y1`/`x2`/`y2`, as fractions of the box

## Effects: how a clip looks and sounds

Effects are listed in `effects` on a clip, a track or the stack. The player
applies a clip's own effects first, then its track's. On the stack it only
grades `ColorLut`s over the whole image and uses a `MotionBlur` as every
clip's default.
There are three families.

### 1. Tellers effects (`tellers.ai.<Effect>`)

Filters, transforms, masks, blend modes, audio volume and pan, speed, and
text animations all share one shape:

```json
{
  "OTIO_SCHEMA": "Effect.1",
  "effect_name": "tellers.ai.Transform",
  "enabled": true,
  "metadata": { "tellers.ai": { "parameters": {
    "x": 0.1,
    "opacity": { "keyframes": [ { "t": 0, "v": 0 }, { "t": 0.5, "v": 1, "ease": "easeOut" } ] }
  } } }
}
```

Read it as: *move the clip right by a tenth of the frame width, and fade it
in over its first half second.*

- A **numeric parameter** is a number, or `{ "keyframes": [...] }`. Each
  keyframe is `{t, v, ease?}`. Before the first keyframe the value is the
  first one's; after the last, the last one's.
- **Strings** (colors such as `#00ff00`, presets, modes) and **booleans** are
  fixed values.
- `"enabled": false` turns an effect off. Names the player doesn't know are
  ignored.
- Older files may use bare names (`Blur` rather than `tellers.ai.Blur`).
  They mean the same thing.

The [reference](otio-reference.md#tellers-effects) lists every effect and its
parameters.

### 2. Resolve effects (`effect_name: "Resolve Effect"`)

These are kept in DaVinci Resolve's own format, in `metadata.Resolve_OTIO`.
Each has `Effect Name` and `Parameters: [{ "Parameter ID", "Parameter Value", … }]`.
The player reads three of them:

| `Effect Name` | Parameters | Meaning |
|---|---|---|
| `Transform` | `transformationPan`, `transformationTilt`, `transformationZoomX/Y`, `transformationRotationAngle` | Places a **video or image** clip: pan and tilt are offsets from the center in frame widths and heights, zoom multiplies the size, and rotation is in degrees. |
| `Cropping` | crop insets | Crops the picture. |
| `Fairlight Clip Volume and Fades` | `volume` (dB) | Legacy clip volume, read only when the clip's `tellers.ai.Volume` sets no `volume` (that one wins). Never written. |

Text clips are placed by the tellers `Transform` effect instead. Shape clips
take it too, on top of any Resolve `Transform` and `Cropping`. A
Rich Text clip uses its own `position`, with the tellers `Transform` applied
on top.

### 3. OTIO time effects

`LinearTimeWarp.1` with `time_scalar` changes playback speed (2 is twice as
fast). `FreezeFrame.1` holds a single frame. They work like
`tellers.ai.TimeWarp`. The player applies the first enabled one of the three
and clamps the speed to 0.25–4 in magnitude. A clip keeps its duration on the
timeline and plays the integral of its speed from `source_range.start_time`.
A negative speed plays that same source range backwards, from
`start_time + |∫ speed|` down to `start_time`; a keyframed speed may be
negative throughout but must not cross zero, else the player ignores it. A freeze plays no media and is
silent. `preserve_pitch: false` (in `metadata["tellers.ai"].parameters`)
resamples the audio instead of time-stretching it.

`Clip.get_speed` reads the speed; `Clip.is_reversed` and
`Clip.source_time_at` (Python `get_source_time_at`) say which way it plays and
which source time shows at a timeline offset. `Clip.set_speed` writes a
constant one (0 for a freeze, negative for reverse) as `LinearTimeWarp.1` / `FreezeFrame.1`, replacing any other
time effect. `Stack.set_item_speed` does the same on a clip's sync partners.
Splitting a clip and clamping it to its media count the media it plays at
that speed; split in two, a reversed clip's left half keeps the top of its
source range.

Only video and audio clips play at a speed. Edits refuse a time effect on a
track, the timeline, a composite clip or anything inside one, and on a Text,
Shape or Rich Text clip (`Clip.set_speed` raises for anything but 1), as well
as a speed the player would clamp or ignore. `validate_timeline` reports such
speed in a loaded document.

### Colour grading (`tellers.ai.ColorLut`)

A `.cube` LUT, an ordinary tellers effect identified by its `asset_id`
parameter; `target_url`, the file the player downloads, is filled in from it
before playback, like a media reference's `target_url` and like any effect with
an `asset_id` (`cube_url`, its former name, is still read). It grades a
clip, every clip of a track, or the whole image when it is on the stack.
Clip LUTs apply first, then the track's, then the timeline's.

## Ids and `tellers.ai` metadata

| Where | Key under `metadata["tellers.ai"]` | Meaning |
|---|---|---|
| tracks, clips, gaps, markers | `timeline_id` | Stable id that edits use to address the object. |
| clips | `group_id` | The player group the clip belongs to (see `groups`). The library's edit groups use `Tellers Group ID` instead. |
| timeline | `groups` | `{ <group_id>: { parameters } }`: a keyframable transform (`x`, `y`, `scale`, `scaleX`, `scaleY`, `rotation`, `opacity`, `pivotX`, `pivotY`) applied to the group's clips and composite clips together. Keyframe `t` counts from the earliest member's start. A `group_id` missing here leaves its clips ungrouped. |
| tracks | `created_by: "auto"` | The library created the track; `prune_empty_partner_tracks` removes it once it is an empty audio track. |
| markers | `created_by` | The user who left the note. |
| timeline | `availableFonts` | `[{family, url, weight?, style?}]` or `[{family, asset_id, target_url?, weight?, style?}]` fonts that text clips may use: linked by URL, or uploaded as an asset whose `target_url` is resolved before playback and never saved (see [Asset references](otio-reference.md#asset-references)). |

The player also reads `tellers_aspect_ratio` (`16_9`, `9_16`, …) directly in
the timeline's `metadata`: the output aspect ratio, 16:9 when absent. Any other
metadata is kept as it is.

## Other objects the player understands

- **Transitions** (`Transition.1`) sit between two items of a track and blend
  them across the cut. They take no time of their own: `in_offset` and
  `out_offset` say how far they reach into the item before and the item
  after. `transition_type` can be `Dissolve`, `DipToBlack`, `DipToWhite`,
  `DipToColor`, `Wipe`, `Push`, `Slide` or `Zoom`. A `direction` and a
  `color` are set in `metadata["tellers.ai"].parameters`.
- **Audio transitions:** sound fades over every transition, on audio tracks
  as on video ones: two clips crossfade, and a clip next to a gap fades from
  or to silence. `transition_type` `tellers.ai.audio-constant-power` (equal
  power) or `tellers.ai.audio-constant-gain` (linear) picks the curve; any
  other type fades at constant power between two clips and at constant gain
  from or to silence. A picture draws an audio type as a dissolve.
- **Composite clips:** a `Stack.1` placed inside a video track is drawn as
  one clip made of its own video tracks. Inside it, the player draws video,
  image, Text and Shape clips; it skips Rich Text clips and transitions.

> **Library support.** `tellers-timeline-core` loads, round-trips and edits
> both transitions and composite clips, and reads and edits clip groups
> (`groups` and each member's `group_id`). A transition's `direction` and
> `color`, and `tellers_aspect_ratio`, have no typed API: they are kept as
> plain metadata.

## A worked example

```json
{
  "OTIO_SCHEMA": "Timeline.1",
  "name": "Promo",
  "metadata": { "tellers.ai": { "availableFonts": [ { "family": "Inter", "url": "https://cdn.example.test/inter.woff2" } ] } },
  "tracks": {
    "OTIO_SCHEMA": "Stack.1",
    "children": [
      {
        "OTIO_SCHEMA": "Track.1", "kind": "video", "name": "V1",
        "children": [
          {
            "OTIO_SCHEMA": "Clip.2", "name": "beach",
            "source_range": { "OTIO_SCHEMA": "TimeRange.1",
              "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 12, "rate": 1 },
              "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": 5,  "rate": 1 } },
            "media_references": { "DEFAULT_MEDIA": { "OTIO_SCHEMA": "ExternalReference.1", "target_url": "" } },
            "effects": [
              { "OTIO_SCHEMA": "Effect.1", "effect_name": "tellers.ai.Adjustment",
                "metadata": { "tellers.ai": { "parameters": { "saturation": 1.3 } } } }
            ],
            "metadata": { "tellers.ai": { "timeline_id": "a1b2c3d4e5f6" } }
          }
        ]
      },
      {
        "OTIO_SCHEMA": "Track.1", "kind": "video", "name": "V2",
        "children": [
          { "OTIO_SCHEMA": "Gap.1",
            "source_range": { "OTIO_SCHEMA": "TimeRange.1",
              "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
              "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": 1, "rate": 1 } } },
          {
            "OTIO_SCHEMA": "Clip.2", "name": "title",
            "source_range": { "OTIO_SCHEMA": "TimeRange.1",
              "start_time": { "OTIO_SCHEMA": "RationalTime.1", "value": 0, "rate": 1 },
              "duration":   { "OTIO_SCHEMA": "RationalTime.1", "value": 3, "rate": 1 } },
            "media_references": { "DEFAULT_MEDIA": {
              "OTIO_SCHEMA": "GeneratorReference.1", "generator_kind": "Text",
              "parameters": { "text": "Summer is here", "style": { "fontFamily": "Inter", "fontSize": 96, "fill": "#ffffff" } } } },
            "effects": [
              { "OTIO_SCHEMA": "Effect.1", "effect_name": "tellers.ai.Transform",
                "metadata": { "tellers.ai": { "parameters": { "y": 0.3 } } } },
              { "OTIO_SCHEMA": "Effect.1", "effect_name": "tellers.ai.TextTypewriter",
                "metadata": { "tellers.ai": { "parameters": {
                  "progress": { "keyframes": [ { "t": 0, "v": 0 }, { "t": 1, "v": 1 } ] } } } } }
            ]
          }
        ]
      }
    ]
  }
}
```

Reading it:

1. **Length.** `V1` is one 5 s clip and `V2` is 1 s + 3 s, so the timeline
   lasts 5 s.
2. **`V1`, the bottom layer.** It plays the `beach` file from its 12th to its
   17th second, between 0 s and 5 s on the timeline, with 30% more
   saturation.
3. **`V2`, drawn on top.** It has nothing for the first second. From 1 s to
   4 s it shows the text "Summer is here" in white 96 px Inter, moved down by
   30% of the frame height.
4. **The typewriter.** Its keyframes run from `t = 0` to `t = 1`, and `t` is
   measured from the clip start. The letters therefore appear one by one
   between 1 s and 2 s on the timeline.
5. **No sound.** There is no audio track, so nothing is heard.
6. **The empty `target_url`.** It is normal in a stored file: tellers fills
   in the media URL from the asset before playback.

## Inspecting a file from Python

With the Python bindings, this prints each track's items with their timeline
times (here, on the example above):

```python
from tellers_timeline import Timeline

tl = Timeline.parse_json(open("promo.otio").read())
for track in tl.get_stack().tracks():
    start = 0.0
    print(track.kind, track.get_name())
    for item in track.get_items():
        end = start + item.duration()
        kind = "clip" if item.is_clip() else "gap"
        print(f"  {start:5.2f}s – {end:5.2f}s  {kind}  {item.get_id()}")
        start = end
print(tl.get_tellers_effects())                    # effects on the stack
print(tl.get_item_tellers_effects("a1b2c3d4e5f6"))  # effects on one clip, by id
```

```
video V1
   0.00s –  5.00s  clip  a1b2c3d4e5f6
video V2
   0.00s –  1.00s  gap  None
   1.00s –  4.00s  clip  None
[]
[{'effect_name': 'Adjustment', 'name': None, 'enabled': True, 'parameters': {'saturation': 1.3}}]
```

Tellers effect names come back bare (`Adjustment`), whichever way they were
written.
