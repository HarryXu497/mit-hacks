# Google Research Football: camera and stadium notes

Research date: 2026-09-19. This is a reference study, not an implementation change.
The existing geometric jungle build is preserved in commit `0f4044c`.

## Direction to borrow

Make the scene feel like a football ground inside a jungle, with room around the players and a camera overlooking the touchline. Keep our original geometric monkeys, warm lighting, and jungle setting. Borrow the reference's spatial clarity, not its realistic assets or simulation.

## What the source actually does

Inspected Google Research Football revision `3d9e754720a95621bba6475c4d3b0d56fe919014`.

- The default in-game camera is the `wide` branch: an elevated sideline broadcast view with perspective and a narrow field of view. It follows the action instead of promising both goals in frame at all times.
- Its target blends 40% ball position with 60% designated possession-player position, then adds player-direction and attack-direction offsets. This gives space ahead of play rather than centering the ball mechanically.
- The target is clamped and smoothed using a weighted history of up to 150 samples. That is a sample count, not a verified duration in seconds.
- With the default settings and a centered target, the wide-camera equations yield roughly 58 units of sideline offset, 29 units of elevation, and a field-of-view parameter around 21.5 degrees. These are reference-engine values, not Bevy settings to paste verbatim; axis and projection conventions need checking during implementation.
- Bird's-eye and telephoto branches exist, but `camMethod = 1` selects wide in the inspected source. These are not evidence of a user-facing camera-mode menu.

Source: [camera defaults](https://github.com/google-research/football/blob/3d9e754720a95621bba6475c4d3b0d56fe919014/third_party/gfootball_engine/src/gamedefines.hpp#L44), [camera targeting and projection parameters](https://github.com/google-research/football/blob/3d9e754720a95621bba6475c4d3b0d56fe919014/third_party/gfootball_engine/src/onthepitch/match.cpp#L499).

## Pitch scale versus stadium scale

| Measurement | Google reference | Current jungle scene |
| --- | --- | --- |
| Pitch | 110 x 72 engine units | 36 x 24 playable field |
| Pitch aspect ratio | 1.528 | 1.500; marked rectangle approximately 1.519 |
| Pitch including reference rim | 120 x 80 | Not directly comparable to our side extensions |
| Goal opening | 7.4 wide x 2.5 high | 8 wide x 5 high |
| Goal width / field cross-width | About 10.3% | 22.2% |

The important finding: the field shape remains close, and the new dimensions give the chunky players more breathing room. The goals are still intentionally expressive rather than realistic, but no longer consume as much of the field's visual width.

The reference pitch rim leaves 5 units behind each goal and 4 units beside each touchline. That separation is worth borrowing: pitch, clear runoff, then spectators and scenery.

These pitch measurements are not the outer stadium dimensions. The rendered stadium is a separate mesh. Its object file also declares a physics box; that is not a trustworthy measurement of the visible stadium bowl. Exact outer seating dimensions were not established in this study.

Source: [pitch and goal constants](https://github.com/google-research/football/blob/3d9e754720a95621bba6475c4d3b0d56fe919014/third_party/gfootball_engine/src/gamedefines.hpp#L244), [stadium object](https://github.com/google-research/football/blob/3d9e754720a95621bba6475c4d3b0d56fe919014/third_party/gfootball_engine/data/media/objects/stadiums/test/test.object).

## Recommended translation to our scene

1. Use the elevated broadcast view as the permanent presentation camera: gentle perspective, restrained ball/player tracking, and smooth motion. The current camera sits at a 30-unit height and 42-unit distance from its smoothed target, with a 38-degree vertical field of view.
2. Because the broadcast view can crop distant ground, preserve the clear pitch markings, readable goals, and team silhouettes at its closest framing. Do not add foreground foliage that can hide the ball or players.
3. Give the pitch more of the image. A useful initial art-direction target is roughly 75–85% of screen width, with enough margin for both goals. This is a proposed composition target, not a number taken from Google.
4. Establish three readable layers: unobstructed playing surface; low, clear runoff and boundary props; taller jungle terraces, huts, cliffs, and waterfall backdrop. Put foreground leaves outside the ball/player sightlines.
5. Suggest stadium capacity through repeated low-poly spectator terraces and horizontal rows of banners. Broader surrounding structures can create scale without changing gameplay dimensions.
6. Keep the chunky monkeys readable. Do not shrink everything to realistic football proportions: their expressions and team colors are part of this game's appeal. Test player silhouette and ball visibility at the actual output resolution.
7. Keep the scoreboard outside the tracked playing area. For a moving camera, decide separately whether the existing world-space board stays an environmental prop or score information also needs a screen-space display.

## RL boundary and future decisions

The camera, level colliders, field size, and goal size are now implemented from this follow-up. Reward, observation, action, and training code were not changed. Field and goal dimensions are gameplay geometry and must be synchronized with the parallel training work.

Changing the true field or goal dimensions affects the training task and must be coordinated with the teammate. The decorative field and the collision boundaries now agree; do not create a second set of geometry constants in the training integration.

If agents consume rendered pixels, even a camera or art change alters their observations; confirm whether training uses state vectors or pixels before connecting this presentation to training.

The implementation now prioritizes the closer action-following broadcast presentation permanently; the full-field overview remains a useful future debug camera if agent inspection later requires it.

Additional reference: [Google's introduction and gameplay examples](https://research.google/blog/introducing-google-research-football-a-novel-reinforcement-learning-environment/).
